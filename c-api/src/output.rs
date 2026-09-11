use super::*;
use pdf_inspector::TextItem;
use std::any::Any;

#[derive(Default)]
pub(super) struct Storage {
    allocations: Vec<Box<dyn Any>>,
}
impl Storage {
    fn keep<T: 'static>(&mut self, values: Vec<T>) -> (*const T, usize) {
        let values = values.into_boxed_slice();
        let pair = (values.as_ptr(), values.len());
        self.allocations.push(Box::new(values));
        pair
    }
    pub(super) fn bytes(&mut self, bytes: impl Into<Vec<u8>>) -> PdfBytes {
        let (ptr, len) = self.keep(bytes.into());
        PdfBytes { ptr, len }
    }
    pub(super) fn optional(&mut self, text: Option<&str>) -> PdfBytes {
        text.map_or(PdfBytes::default(), |text| self.bytes(text.as_bytes()))
    }
    pub(super) fn strings(
        &mut self,
        text: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> PdfStrings {
        let strings = text
            .into_iter()
            .map(|s| self.bytes(s.as_ref().as_bytes()))
            .collect();
        let (ptr, len) = self.keep(strings);
        PdfStrings { ptr, len }
    }
    /// Items arrive y-up lower-left in the request frame from core; present
    /// y-down top-left with clockwise rotation (presentation only, no math).
    pub(super) fn item(&mut self, item: &TextItem, frame_height: f32) -> PdfItem {
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
            text: self.bytes(item.text.as_bytes()),
            font: self.bytes(item.font.as_bytes()),
            font_tag: self.bytes(item.font_tag.as_bytes()),
            link: self.optional(link),
        }
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

impl Storage {
    pub(super) fn boxes(&mut self, v: Vec<PdfBox>) -> PdfBoxes {
        let (ptr, len) = self.keep(v);
        PdfBoxes { ptr, len }
    }
    pub(super) fn items(&mut self, v: Vec<PdfItem>) -> PdfItems {
        let (ptr, len) = self.keep(v);
        PdfItems { ptr, len }
    }
    pub(super) fn structure_elements(
        &mut self,
        v: Vec<PdfStructureElement>,
    ) -> PdfStructureElements {
        let (ptr, len) = self.keep(v);
        PdfStructureElements { ptr, len }
    }
    pub(super) fn rectangles(&mut self, v: Vec<PdfRectangle>) -> PdfRectangles {
        let (ptr, len) = self.keep(v);
        PdfRectangles { ptr, len }
    }
    pub(super) fn segments(&mut self, v: Vec<PdfSegment>) -> PdfSegments {
        let (ptr, len) = self.keep(v);
        PdfSegments { ptr, len }
    }
    pub(super) fn pages(&mut self, v: Vec<PdfPage>) -> PdfPages {
        let (ptr, len) = self.keep(v);
        PdfPages { ptr, len }
    }
    pub(super) fn regions(&mut self, v: Vec<PdfRegion>) -> PdfRegions {
        let (ptr, len) = self.keep(v);
        PdfRegions { ptr, len }
    }
    pub(super) fn tables(&mut self, v: Vec<PdfTable>) -> PdfTables {
        let (ptr, len) = self.keep(v);
        PdfTables { ptr, len }
    }
    pub(super) fn cells(&mut self, v: Vec<PdfCell>) -> PdfCells {
        let (ptr, len) = self.keep(v);
        PdfCells { ptr, len }
    }
    pub(super) fn structure_nodes(&mut self, v: Vec<PdfStructureNode>) -> PdfStructureNodes {
        let (ptr, len) = self.keep(v);
        PdfStructureNodes { ptr, len }
    }
    pub(super) fn content_references(
        &mut self,
        v: Vec<PdfContentReference>,
    ) -> PdfContentReferences {
        let (ptr, len) = self.keep(v);
        PdfContentReferences { ptr, len }
    }
}
