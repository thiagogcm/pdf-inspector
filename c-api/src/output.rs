use super::*;
use pdf_inspector::{BoldSource, PageRotation, TextItem};

/// Memory behind one published record graph, freed together. Small strings
/// and record arrays live in the arena; large owned buffers are kept as is.
#[derive(Default)]
pub(super) struct Storage {
    arena: bumpalo::Bump,
    owned: std::cell::RefCell<Vec<Vec<u8>>>,
}
/// A published `{ ptr, len }` record over `Item`s.
pub(super) trait CSlice {
    type Item: Copy;
    fn from_raw(ptr: *const Self::Item, len: usize) -> Self;
}
macro_rules! c_slices {
    ($($t:ident => $item:ty;)*) => {$(
        impl CSlice for $t {
            type Item = $item;
            fn from_raw(ptr: *const $item, len: usize) -> Self {
                Self { ptr, len }
            }
        }
    )*};
}
c_slices! {
    PdfStrings => PdfBytes;
    PdfBoxes => PdfBox;
    PdfIntervals => PdfInterval;
    PdfItems => PdfItem;
    PdfPageInfos => PdfPageInfo;
    PdfStructureElements => PdfStructureElement;
    PdfRectangles => PdfRectangle;
    PdfSegments => PdfSegment;
    PdfPages => PdfPage;
    PdfRegions => PdfRegion;
    PdfTables => PdfTable;
    PdfCells => PdfCell;
    PdfStructureNodes => PdfStructureNode;
    PdfContentReferences => PdfContentReference;
    PdfCMapGaps => PdfCMapGap;
}
impl Storage {
    pub(super) fn bytes(&self, bytes: impl AsRef<[u8]>) -> PdfBytes {
        let copy = self.arena.alloc_slice_copy(bytes.as_ref());
        PdfBytes {
            ptr: copy.as_ptr(),
            len: copy.len(),
        }
    }
    /// Keep a large buffer without copying it.
    pub(super) fn owned(&self, bytes: Vec<u8>) -> PdfBytes {
        let view = PdfBytes {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };
        self.owned.borrow_mut().push(bytes);
        view
    }
    pub(super) fn optional(&self, text: Option<&str>) -> PdfBytes {
        text.map_or(PdfBytes::default(), |text| self.bytes(text))
    }
    /// Collect straight into the arena; the iterator may allocate here too.
    pub(super) fn slice<S: CSlice>(&self, values: impl IntoIterator<Item = S::Item>) -> S {
        let copy = bumpalo::collections::Vec::from_iter_in(values, &self.arena).into_bump_slice();
        S::from_raw(copy.as_ptr(), copy.len())
    }
    pub(super) fn strings(&self, text: impl IntoIterator<Item = impl AsRef<str>>) -> PdfStrings {
        self.slice(text.into_iter().map(|s| self.bytes(s.as_ref())))
    }
    /// Items arrive y-up lower-left in the request frame from core; present
    /// y-down top-left with clockwise rotation (presentation only, no math).
    pub(super) fn item(&self, item: &TextItem, frame_height: f32) -> PdfItem {
        let flags = (u32::from(item.is_bold) * PDF_BOLD)
            | (u32::from(item.is_italic) * PDF_ITALIC)
            | (u32::from(item.is_underline) * PDF_UNDERLINE)
            | (u32::from(item.is_strikeout) * PDF_STRIKEOUT)
            | (u32::from(item.mcid.is_some()) * PDF_HAS_MCID)
            | (u32::from(item.advance_known) * PDF_ADVANCE_KNOWN)
            | (u32::from(item.legacy_symbol_rewrite) * PDF_LEGACY_SYMBOL_REWRITE);
        let (kind, link) = match &item.item_type {
            pdf_inspector::types::ItemType::Text => (PDF_ITEM_TEXT, None),
            pdf_inspector::types::ItemType::Image => (PDF_ITEM_IMAGE, None),
            pdf_inspector::types::ItemType::Link(s) => (PDF_ITEM_LINK, Some(s.as_str())),
            pdf_inspector::types::ItemType::FormField => (PDF_ITEM_FORM_FIELD, None),
        };
        PdfItem {
            page: item.page,
            kind,
            flags,
            bounds: flip_box(item.x, item.y, item.width, item.height, frame_height),
            font_size: item.font_size,
            rotation: (-item.rotation).rem_euclid(360.0),
            baseline_shift: item.baseline_shift,
            mcid: item.mcid.unwrap_or(0),
            font_weight: item.font_weight.map(u32::from).unwrap_or(0),
            bold_source: bold_source(item.bold_source),
            fixed_pitch: match item.fixed_pitch {
                None => PDF_PITCH_UNKNOWN,
                Some(true) => PDF_PITCH_FIXED,
                Some(false) => PDF_PITCH_PROPORTIONAL,
            },
            text: self.bytes(&item.text),
            font: self.bytes(&item.font),
            font_tag: self.bytes(&item.font_tag),
            dest_page: 0,
            link: self.optional(link),
        }
    }
}
fn bold_source(source: Option<BoldSource>) -> u32 {
    match source {
        None => 0,
        Some(BoldSource::FontName) => PDF_BOLD_FONT_NAME,
        Some(BoldSource::FontFlags) => PDF_BOLD_FONT_FLAGS,
        Some(BoldSource::WeightClass) => PDF_BOLD_WEIGHT_CLASS,
        Some(BoldSource::Painted) => PDF_BOLD_PAINTED,
    }
}
pub(super) fn parse_font_weight(value: u32) -> Fallible<Option<u16>> {
    match value {
        0 => Ok(None),
        100..=900 => Ok(Some(value as u16)),
        _ => Err(Failure::invalid("font weight is outside 100..900")),
    }
}
pub(super) fn parse_bold_source(value: u32) -> Fallible<Option<BoldSource>> {
    match value {
        0 => Ok(None),
        PDF_BOLD_FONT_NAME => Ok(Some(BoldSource::FontName)),
        PDF_BOLD_FONT_FLAGS => Ok(Some(BoldSource::FontFlags)),
        PDF_BOLD_WEIGHT_CLASS => Ok(Some(BoldSource::WeightClass)),
        PDF_BOLD_PAINTED => Ok(Some(BoldSource::Painted)),
        _ => Err(Failure::invalid("unknown bold source")),
    }
}
pub(super) fn parse_fixed_pitch(value: u32) -> Fallible<Option<bool>> {
    match value {
        PDF_PITCH_UNKNOWN => Ok(None),
        PDF_PITCH_FIXED => Ok(Some(true)),
        PDF_PITCH_PROPORTIONAL => Ok(Some(false)),
        _ => Err(Failure::invalid("unknown fixed pitch")),
    }
}
/// y-up lower-left box to y-down top-left (presentation flip, no rotation).
pub(super) fn flip_box(x: f32, y: f32, w: f32, h: f32, frame_height: f32) -> PdfBox {
    let (x0, x1) = (x.min(x + w), x.max(x + w));
    let (y0, y1) = (y.min(y + h), y.max(y + h));
    PdfBox {
        x0,
        y0: frame_height - y1,
        x1,
        y1: frame_height - y0,
    }
}
/// y-up point to y-down top-left.
pub(super) fn flip_point(x: f32, y: f32, frame_height: f32) -> PdfPoint {
    PdfPoint {
        x,
        y: frame_height - y,
    }
}
pub(super) fn page_box(v: [f32; 4]) -> PdfBox {
    PdfBox {
        x0: v[0],
        y0: v[1],
        x1: v[2],
        y1: v[3],
    }
}
impl PdfTransform {
    #[cfg(test)]
    pub(super) fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
    #[cfg(feature = "render-pdfium")]
    pub(super) fn inverse(self) -> Fallible<Self> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < f64::EPSILON {
            return Err(Failure::invalid("noninvertible image transform"));
        }
        Ok(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            e: (self.c * self.f - self.d * self.e) / det,
            f: (self.b * self.e - self.a * self.f) / det,
        })
    }
}

/// Core `usize` counts and indices as C `uint32_t`, saturating.
pub(super) fn narrow(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
pub(super) fn orientation(rotation: PageRotation) -> u32 {
    match rotation {
        PageRotation::Upright => PDF_ORIENTATION_UPRIGHT,
        PageRotation::Ccw => PDF_ORIENTATION_CCW,
        PageRotation::Cw => PDF_ORIENTATION_CW,
    }
}
