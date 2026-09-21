use super::*;
use pdf_inspector::vision::{OcrMode, RenderOptions, RenderPixelFormat};
use pdf_inspector::{MarkdownOptions, MarkdownProfile, PositionFrame, PositionOptions};
use std::collections::BTreeSet;

pub(super) unsafe fn required<'a, T>(ptr: *const T, name: &str) -> Fallible<&'a T> {
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<T>()) {
        return Err(Failure::invalid(format!("{name} is NULL or misaligned")));
    }
    Ok(&*ptr)
}
pub(super) unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> Fallible<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if len > isize::MAX as usize / std::mem::size_of::<T>()
        || ptr.is_null()
        || !(ptr as usize).is_multiple_of(std::mem::align_of::<T>())
    {
        return Err(Failure::invalid("invalid array pointer or length"));
    }
    Ok(std::slice::from_raw_parts(ptr, len))
}
pub(super) unsafe fn bytes<'a>(view: PdfBytes) -> Fallible<&'a [u8]> {
    slice(view.ptr, view.len)
}
pub(super) unsafe fn text<'a>(view: PdfBytes) -> Fallible<&'a str> {
    std::str::from_utf8(bytes(view)?).map_err(|_| Failure::invalid("invalid UTF-8"))
}
pub(super) unsafe fn optional_text<'a>(view: PdfBytes) -> Fallible<Option<&'a str>> {
    let value = text(view)?;
    Ok((!view.ptr.is_null()).then_some(value))
}
pub(super) unsafe fn path<'a>(view: PdfBytes) -> Fallible<&'a str> {
    let s = text(view)?;
    if s.is_empty() || s.contains('\0') {
        return Err(Failure::invalid("path is empty or contains NUL"));
    }
    Ok(s)
}
pub(super) fn finite(v: f32, name: &str) -> Fallible<f32> {
    if !v.is_finite() {
        Err(Failure::invalid(format!("{name} must be finite")))
    } else {
        Ok(v)
    }
}
pub(super) fn ratio(v: f32, name: &str) -> Fallible<f32> {
    finite(v, name)?;
    if !(0.0..=1.0).contains(&v) {
        Err(Failure::invalid(format!("{name} must be in [0,1]")))
    } else {
        Ok(v)
    }
}
pub(super) fn bounds(v: PdfBox) -> Fallible<[f32; 4]> {
    for n in [v.x0, v.y0, v.x1, v.y1] {
        finite(n, "bounds")?;
    }
    if v.x1 <= v.x0 || v.y1 <= v.y0 || !(v.x1 - v.x0).is_finite() || !(v.y1 - v.y0).is_finite() {
        return Err(Failure::invalid(
            "bounds must have positive area and ordered corners",
        ));
    }
    Ok([v.x0, v.y0, v.x1, v.y1])
}
pub(super) fn page(page: u32, count: u32) -> Fallible<()> {
    if page == 0 || page > count {
        Err(Failure::invalid(format!(
            "page {page} is outside 1..={count}"
        )))
    } else {
        Ok(())
    }
}
pub(super) unsafe fn pages(v: PdfPageNumbers, count: u32) -> Fallible<Vec<u32>> {
    let pages = slice(v.ptr, v.len)?;
    if pages.is_empty() {
        return Ok((1..=count).collect());
    }
    pages.iter().try_for_each(|p| page(*p, count))?;
    Ok(pages
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}
pub(super) fn default_markdown() -> PdfMarkdownOptions {
    let m = MarkdownOptions::default();
    let pairs = [
        (m.detect_headers, PDF_MD_HEADERS),
        (m.detect_lists, PDF_MD_LISTS),
        (m.detect_code, PDF_MD_CODE),
        (m.remove_page_numbers, PDF_MD_REMOVE_PAGE_NUMBERS),
        (m.format_urls, PDF_MD_URLS),
        (m.fix_hyphenation, PDF_MD_HYPHENATION),
        (m.detect_bold, PDF_MD_BOLD),
        (m.detect_italic, PDF_MD_ITALIC),
        (m.detect_underline, PDF_MD_UNDERLINE),
        (m.include_images, PDF_MD_IMAGES),
        (m.include_links, PDF_MD_LINKS),
        (m.include_page_numbers, PDF_MD_PAGE_NUMBERS),
        (m.strip_headers_footers, PDF_MD_STRIP_FURNITURE),
    ];
    PdfMarkdownOptions {
        flags: pairs.into_iter().filter(|p| p.0).fold(0, |a, p| a | p.1),
        profile: match m.profile {
            MarkdownProfile::Compact => PDF_PROFILE_COMPACT,
            MarkdownProfile::Fidelity => PDF_PROFILE_FIDELITY,
        },
        base_font_size: 0.0,
    }
}
pub(super) fn markdown(m: &PdfMarkdownOptions) -> Fallible<MarkdownOptions> {
    if m.flags & !8191 != 0 {
        return Err(Failure::invalid("unknown Markdown flags"));
    }
    finite(m.base_font_size, "base font size")?;
    if m.base_font_size < 0.0 {
        return Err(Failure::invalid("base font size must be nonnegative"));
    }
    Ok(MarkdownOptions {
        profile: match m.profile {
            PDF_PROFILE_COMPACT => MarkdownProfile::Compact,
            PDF_PROFILE_FIDELITY => MarkdownProfile::Fidelity,
            _ => return Err(Failure::invalid("unknown Markdown profile")),
        },
        base_font_size: (m.base_font_size > 0.0).then_some(m.base_font_size),
        detect_headers: m.flags & PDF_MD_HEADERS != 0,
        detect_lists: m.flags & PDF_MD_LISTS != 0,
        detect_code: m.flags & PDF_MD_CODE != 0,
        remove_page_numbers: m.flags & PDF_MD_REMOVE_PAGE_NUMBERS != 0,
        format_urls: m.flags & PDF_MD_URLS != 0,
        fix_hyphenation: m.flags & PDF_MD_HYPHENATION != 0,
        detect_bold: m.flags & PDF_MD_BOLD != 0,
        detect_italic: m.flags & PDF_MD_ITALIC != 0,
        detect_underline: m.flags & PDF_MD_UNDERLINE != 0,
        include_images: m.flags & PDF_MD_IMAGES != 0,
        include_links: m.flags & PDF_MD_LINKS != 0,
        include_page_numbers: m.flags & PDF_MD_PAGE_NUMBERS != 0,
        strip_headers_footers: m.flags & PDF_MD_STRIP_FURNITURE != 0,
    })
}
pub(super) fn default_request() -> PdfRequest {
    let r = RenderOptions::default();
    let o = pdf_inspector::vision::OcrOptions::default();
    let d = pdf_inspector::DetectionConfig::default();
    PdfRequest {
        outputs: PDF_INSPECTION | PDF_MARKDOWN,
        markdown: default_markdown(),
        detection: PdfDetectionOptions {
            strategy: PDF_SCAN_SAMPLE,
            sample_size: 8,
            min_text_ops: d.min_text_ops_per_page,
            text_page_ratio: d.text_page_ratio_threshold,
            pages: PdfPageNumbers::default(),
        },
        render: PdfRenderOptions {
            dpi: r.dpi,
            format: PDF_RGB8,
            annotations: u32::from(r.annotations),
            form_fields: u32::from(r.form_fields),
            max_page_bytes: r.max_output_bytes_per_page,
        },
        ocr: PdfOcrOptions {
            mode: PDF_OCR_OFF,
            download_policy: PDF_DOWNLOAD_IF_MISSING,
            minimum_confidence: o.minimum_confidence,
            hosted_confidence: 0.5,
            model_directory: PdfBytes::default(),
        },
        bold_weight_threshold: 600,
        ..PdfRequest::default()
    }
}
pub(super) fn render(r: &PdfRenderOptions) -> Fallible<RenderOptions> {
    finite(r.dpi, "DPI")?;
    if r.dpi <= 0.0 || r.annotations > 1 || r.form_fields > 1 || r.max_page_bytes == 0 {
        return Err(Failure::invalid("invalid rendering options"));
    }
    let pixel_format = match r.format {
        PDF_RGB8 => RenderPixelFormat::Rgb8,
        PDF_RGBA8 => RenderPixelFormat::Rgba8,
        PDF_GRAY8 => RenderPixelFormat::Gray8,
        _ => return Err(Failure::invalid("unknown pixel format")),
    };
    Ok(RenderOptions {
        dpi: r.dpi,
        pixel_format,
        annotations: r.annotations != 0,
        form_fields: r.form_fields != 0,
        max_output_bytes_per_page: r.max_page_bytes,
    })
}
pub(super) fn ocr_mode(mode: u32) -> Fallible<OcrMode> {
    match mode {
        PDF_OCR_OFF => Ok(OcrMode::Off),
        PDF_OCR_AUTO => Ok(OcrMode::Auto),
        PDF_OCR_FORCE => Ok(OcrMode::Force),
        _ => Err(Failure::invalid("unknown OCR mode")),
    }
}
pub(super) fn position_options(r: &PdfRequest) -> Fallible<PositionOptions> {
    let frame = match r.frame {
        PDF_FRAME_SHEET => PositionFrame::Sheet,
        PDF_FRAME_DISPLAY => PositionFrame::Display,
        _ => return Err(Failure::invalid("unknown coordinate frame")),
    };
    if r.bold_from_weight > 1 {
        return Err(Failure::invalid("unknown bold_from_weight"));
    }
    if !(100..=900).contains(&r.bold_weight_threshold) {
        return Err(Failure::invalid(
            "bold weight threshold is outside 100..900",
        ));
    }
    Ok(PositionOptions::new()
        .frame(frame)
        .bold_from_weight(r.bold_from_weight != 0)
        .bold_weight_threshold(r.bold_weight_threshold as u16))
}
