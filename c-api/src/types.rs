//! Public C records. Geometry defaults to the sheet frame (unrotated
//! visible-page points, y-down); `PdfRequest.frame` selects the display frame
//! for rendered-page coordinates instead.

/// Status codes returned by fallible calls and carried in `PdfError.status`.
pub const PDF_OK: i32 = 0;
pub const PDF_INVALID_ARGUMENT: i32 = 1;
pub const PDF_IO_ERROR: i32 = 2;
pub const PDF_PASSWORD_ERROR: i32 = 3;
pub const PDF_PARSE_ERROR: i32 = 4;
pub const PDF_UNSUPPORTED: i32 = 5;
pub const PDF_RUNTIME_ERROR: i32 = 6;
pub const PDF_PANIC: i32 = 7;

/// `PdfSource.kind`.
pub const PDF_SOURCE_BYTES: u32 = 0;
pub const PDF_SOURCE_PATH: u32 = 1;
/// `PdfRequest.outputs` and `PdfResult.present` bits.
pub const PDF_OUT_INSPECTION: u32 = 1;
pub const PDF_OUT_MARKDOWN: u32 = 2;
pub const PDF_OUT_TEXT: u32 = 4;
pub const PDF_OUT_ITEMS: u32 = 8;
pub const PDF_OUT_STRUCTURE: u32 = 16;
pub const PDF_OUT_GEOMETRY: u32 = 32;
pub const PDF_OUT_RENDER: u32 = 64;
pub const PDF_OUT_ANALYSIS: u32 = 128;
pub const PDF_OUT_TABLES: u32 = 256;
/// `PdfRequest.flags`: also treat a weight class at or above
/// `bold_weight_threshold` as bold.
pub const PDF_REQUEST_BOLD_FROM_WEIGHT: u32 = 1;
/// `PdfRequest.flags`: keep text drawn with render mode 3 (invisible).
pub const PDF_REQUEST_INCLUDE_INVISIBLE: u32 = 2;
/// `PdfRenderOptions.flags` bits.
pub const PDF_RENDER_ANNOTATIONS: u32 = 1;
pub const PDF_RENDER_FORM_FIELDS: u32 = 2;
/// `PdfResult.flags`: a selected page reported suspected garbled text.
pub const PDF_DOC_ENCODING_ISSUES: u32 = 1;
/// `PdfResult.flags`: the detector recommends OCR for the document.
pub const PDF_DOC_OCR_RECOMMENDED: u32 = 2;
/// `PdfRequest.frame`: the visible page box as laid out in the content
/// stream (`/Rotate` not applied).
pub const PDF_FRAME_SHEET: u32 = 0;
/// `PdfRequest.frame`: the visible box turned clockwise by the inheritable
/// `/Rotate`, so region rects can be taken from a page image and items sit
/// where a renderer draws them.
pub const PDF_FRAME_DISPLAY: u32 = 1;
/// `PdfItem.bold_source` when `is_bold` came from a bold word or foundry
/// abbreviation in the font name.
pub const PDF_BOLD_FONT_NAME: u32 = 1;
/// `PdfItem.bold_source` when the FontDescriptor ForceBold flag or the
/// embedded program's bold selection said bold.
pub const PDF_BOLD_FONT_FLAGS: u32 = 2;
/// `PdfItem.bold_source` when `bold_from_weight` credited the weight class.
pub const PDF_BOLD_WEIGHT_CLASS: u32 = 3;
/// `PdfItem.bold_source` when the run was filled and stroked to look heavier.
pub const PDF_BOLD_PAINTED: u32 = 4;
/// `PdfItem.fixed_pitch` when the face does not declare or measure a pitch.
pub const PDF_PITCH_UNKNOWN: u32 = 0;
/// `PdfItem.fixed_pitch` when the face is monospaced.
pub const PDF_PITCH_FIXED: u32 = 1;
/// `PdfItem.fixed_pitch` when the face is proportional.
pub const PDF_PITCH_PROPORTIONAL: u32 = 2;
/// Capability bits: `pdf_inspector_capabilities()`, `PdfRuntimeOptions.capabilities`,
/// and `PdfRuntimeInfo.capabilities`.
pub const PDF_CAP_RENDER: u32 = 1;
pub const PDF_CAP_OCR: u32 = 2;
pub const PDF_CAP_DOWNLOAD: u32 = 4;
pub const PDF_CAP_EXTERNAL_OCR: u32 = 8;
/// `PdfOcrOptions.mode`.
pub const PDF_OCR_OFF: u32 = 0;
pub const PDF_OCR_AUTO: u32 = 1;
pub const PDF_OCR_FORCE: u32 = 2;
/// `PdfOcrOptions.download_policy` and `PdfRuntimeOptions.download_policy`.
pub const PDF_DOWNLOAD_IF_MISSING: u32 = 0;
pub const PDF_DOWNLOAD_OFFLINE: u32 = 1;
/// `PdfRenderOptions.format` and `PdfImage.format`.
pub const PDF_RGB8: u32 = 0;
pub const PDF_RGBA8: u32 = 1;
pub const PDF_GRAY8: u32 = 2;
/// `PdfRegionInput.kind` and `PdfRegion.kind`.
pub const PDF_REGION_TEXT: u32 = 0;
pub const PDF_REGION_TABLE: u32 = 1;
pub const PDF_REGION_GRID: u32 = 2;
/// `PdfTableInput.mode`.
pub const PDF_TSR_AUTO: u32 = 0;
pub const PDF_TSR_STRICT: u32 = 1;
/// `PdfResult.pdf_type`.
pub const PDF_TYPE_TEXT: u32 = 0;
pub const PDF_TYPE_SCANNED: u32 = 1;
pub const PDF_TYPE_IMAGE: u32 = 2;
pub const PDF_TYPE_MIXED: u32 = 3;
/// `PdfDetectionOptions.strategy`.
pub const PDF_SCAN_SAMPLE: u32 = 0;
pub const PDF_SCAN_FULL: u32 = 1;
pub const PDF_SCAN_EARLY_EXIT: u32 = 2;
pub const PDF_SCAN_PAGES: u32 = 3;
/// `PdfMarkdownOptions.profile`.
pub const PDF_PROFILE_COMPACT: u32 = 0;
pub const PDF_PROFILE_FIDELITY: u32 = 1;
/// `PdfItem.kind`.
pub const PDF_ITEM_TEXT: u32 = 0;
pub const PDF_ITEM_IMAGE: u32 = 1;
pub const PDF_ITEM_LINK: u32 = 2;
pub const PDF_ITEM_FORM_FIELD: u32 = 3;
/// `PdfItem.flags` bits.
pub const PDF_BOLD: u32 = 1;
pub const PDF_ITALIC: u32 = 2;
pub const PDF_UNDERLINE: u32 = 4;
pub const PDF_STRIKEOUT: u32 = 8;
pub const PDF_HAS_MCID: u32 = 16;
pub const PDF_ADVANCE_KNOWN: u32 = 32;
/// `PdfItem.flags`: legacy private-use symbol cleanup changed a character.
/// Decoding provenance; absence does not guarantee decoding accuracy.
pub const PDF_LEGACY_SYMBOL_REWRITE: u32 = 64;
/// `PdfPage.flags` bits.
pub const PDF_PAGE_NEEDS_OCR: u32 = 1;
pub const PDF_PAGE_HAS_TABLES: u32 = 2;
pub const PDF_PAGE_HAS_COLUMNS: u32 = 4;
pub const PDF_PAGE_OCR_RAN: u32 = 8;
pub const PDF_PAGE_HOSTED_RECOMMENDED: u32 = 16;
pub const PDF_PAGE_ENCODING_ISSUES: u32 = 32;
pub const PDF_PAGE_GID_ENCODED: u32 = 64;
pub const PDF_PAGE_SKIPPED_INVISIBLE: u32 = 128;
pub const PDF_PAGE_NATIVE_RECOVERED: u32 = 256;
/// `PdfPage.reading_order`.
pub const PDF_READING_SINGLE: u32 = 0;
pub const PDF_READING_TABULAR: u32 = 1;
pub const PDF_READING_NEWSPAPER: u32 = 2;
/// `PdfPage.text_orientation`: no positioned content was parsed.
pub const PDF_ORIENTATION_UNKNOWN: u32 = 0;
/// `PdfPage.text_orientation`: text reads along +x.
pub const PDF_ORIENTATION_UPRIGHT: u32 = 1;
/// `PdfPage.text_orientation`: most runs read bottom-to-top.
pub const PDF_ORIENTATION_CCW: u32 = 2;
/// `PdfPage.text_orientation`: most runs read top-to-bottom.
pub const PDF_ORIENTATION_CW: u32 = 3;
/// `PdfLoadAudit.flags` bits.
pub const PDF_LOAD_DECRYPTED: u32 = 1;
pub const PDF_LOAD_WIDENED_FORM_BBOX: u32 = 2;
pub const PDF_LOAD_LEADING_BYTES: u32 = 4;
pub const PDF_LOAD_CONTAINER_REPAIRED: u32 = 8;
pub const PDF_LOAD_SATURATED_BBOX: u32 = 16;
/// `PdfTable.kind`.
pub const PDF_TABLE_DATA: u32 = 0;
pub const PDF_TABLE_TOC: u32 = 1;
/// `PdfProvenance.source`.
pub const PDF_CONTENT_NATIVE: u32 = 0;
pub const PDF_CONTENT_OCR: u32 = 1;
pub const PDF_CONTENT_FUSED: u32 = 2;
/// Optional-field bits. `PdfOcrPageInput.flags` and `PdfProvenance.flags`
/// take `PDF_HAS_CONFIDENCE` only; `PdfOcrSpan.flags` takes
/// `PDF_HAS_ORIENTATION` only.
pub const PDF_HAS_CONFIDENCE: u32 = 1;
pub const PDF_HAS_ORIENTATION: u32 = 2;
/// `PdfRegion.flags` bits.
pub const PDF_REGION_NEEDS_OCR: u32 = 1;
pub const PDF_REGION_GRID_FOUND: u32 = 2;
/// `PdfCell.flags` bits.
pub const PDF_CELL_HEADER: u32 = 1;
pub const PDF_CELL_HAS_BOUNDS: u32 = 2;
pub const PDF_CELL_SPAN_KNOWN: u32 = 4;
/// `PdfTable.flags` bits.
pub const PDF_TABLE_FROM_HINT: u32 = 1;
pub const PDF_TABLE_HAS_BOUNDS: u32 = 2;
/// `PdfMarkdownOptions.flags` bits.
pub const PDF_MD_HEADERS: u32 = 1;
pub const PDF_MD_LISTS: u32 = 2;
pub const PDF_MD_CODE: u32 = 4;
pub const PDF_MD_REMOVE_PAGE_NUMBERS: u32 = 8;
pub const PDF_MD_URLS: u32 = 16;
pub const PDF_MD_HYPHENATION: u32 = 32;
pub const PDF_MD_BOLD: u32 = 64;
pub const PDF_MD_ITALIC: u32 = 128;
pub const PDF_MD_UNDERLINE: u32 = 256;
pub const PDF_MD_IMAGES: u32 = 512;
pub const PDF_MD_LINKS: u32 = 1024;
pub const PDF_MD_PAGE_NUMBERS: u32 = 2048;
pub const PDF_MD_STRIP_FURNITURE: u32 = 4096;

/// UTF-8 text or binary bytes, never NUL-terminated. NULL/0 denotes absence;
/// non-NULL/0 denotes a present empty value. Output storage belongs to its
/// result, except where a function documents static storage.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfBytes {
    pub ptr: *const u8,
    pub len: usize,
}
impl PdfBytes {
    pub(crate) fn from_static(text: &'static str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }
}
/// `kind` is `PDF_SOURCE_*`. An absent password uses the loader's
/// empty-password behavior.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfSource {
    pub kind: u32,
    pub data: PdfBytes,
    pub password: PdfBytes,
}
/// Axis-aligned bounds in the request's frame (`PDF_FRAME_*`), top-left origin.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPoint {
    pub x: f32,
    pub y: f32,
}
/// Affine mapping: x' = a*x + c*y + e; y' = b*x + d*y + f.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfMarkdownOptions {
    pub flags: u32,
    pub profile: u32,
    pub base_font_size: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfDetectionOptions {
    pub strategy: u32,
    pub sample_size: u32,
    pub min_text_ops: u32,
    pub text_page_ratio: f32,
    pub pages: PdfPageNumbers,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRenderOptions {
    pub dpi: f32,
    pub format: u32,
    pub flags: u32,
    pub max_page_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOcrOptions {
    pub mode: u32,
    pub download_policy: u32,
    pub minimum_confidence: f32,
    pub hosted_confidence: f32,
    pub model_directory: PdfBytes,
}
/// Prepare selected native capabilities without a PDF. Initialized policy is offline.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRuntimeOptions {
    pub capabilities: u32,
    pub download_policy: u32,
    pub model_directory: PdfBytes,
}
/// Written by `pdf_inspector_prepare_runtime`. `capabilities` are the
/// verified `PDF_CAP_*` bits; model identity is filled when OCR was prepared.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRuntimeInfo {
    pub capabilities: u32,
    pub model: PdfBytes,
    pub model_revision: PdfBytes,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRegionInput {
    pub page: u32,
    pub kind: u32,
    pub bounds: PdfBox,
}
/// Cell quadrilateral in page points; four corners in perimeter order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfQuad {
    pub points: [PdfPoint; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfTableInput {
    pub page: u32,
    pub mode: u32,
    pub bounds: PdfBox,
    pub tokens: PdfStrings,
    pub cells: PdfQuads,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOcrSpan {
    pub text: PdfBytes,
    pub polygon: PdfQuad,
    pub confidence: f32,
    pub orientation: f32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOcrPageInput {
    pub page: u32,
    pub flags: u32,
    pub confidence: f32,
    pub processing_ms: u64,
    pub model: PdfBytes,
    pub model_revision: PdfBytes,
    pub warnings: PdfStrings,
    pub spans: PdfOcrSpans,
}
/// Initialize with pdf_inspector_request_init. Empty page selection means all
/// pages. Page lists are sets; query batches preserve input order. `frame`
/// selects the coordinate frame for page dimensions, positioned runs,
/// path geometry, and region rects (see `PDF_FRAME_*`). `flags` are
/// `PDF_REQUEST_*`; `bold_weight_threshold` is the 100..=900 class that
/// `PDF_REQUEST_BOLD_FROM_WEIGHT` treats as bold (600 by default).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRequest {
    pub outputs: u32,
    pub pages: PdfPageNumbers,
    pub markdown: PdfMarkdownOptions,
    pub detection: PdfDetectionOptions,
    pub render: PdfRenderOptions,
    pub ocr: PdfOcrOptions,
    pub regions: PdfRegionInputs,
    pub tables: PdfTableInputs,
    pub external_ocr: PdfOcrPageInputs,
    pub frame: u32,
    pub flags: u32,
    pub bold_weight_threshold: u32,
}
/// Complete positioned run. Rotation is clockwise; positive baseline_shift
/// denotes superscript. MCID is meaningful only when PDF_HAS_MCID is set.
/// PDF_LEGACY_SYMBOL_REWRITE is decoding provenance, not an OCR verdict.
/// `font_weight` is 0 when unknown, else 100..=900. `bold_source` is 0 when
/// `is_bold` is unset, else `PDF_BOLD_FONT_*` / `PDF_BOLD_PAINTED`.
/// `fixed_pitch` is `PDF_PITCH_*`. `dest_page` is a 1-indexed GoTo target;
/// 0 means none. URI stays in `link`. Composition ignores `dest_page`: the
/// Markdown pipeline has no destination concept.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfItem {
    pub page: u32,
    pub kind: u32,
    pub flags: u32,
    pub bounds: PdfBox,
    pub font_size: f32,
    pub rotation: f32,
    pub baseline_shift: f32,
    pub mcid: i64,
    pub font_weight: u32,
    pub bold_source: u32,
    pub fixed_pitch: u32,
    pub dest_page: u32,
    pub text: PdfBytes,
    pub font: PdfBytes,
    pub font_tag: PdfBytes,
    pub link: PdfBytes,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfStructureElement {
    pub page: u32,
    pub mcid: i64,
    pub role: PdfBytes,
}
/// A semantic node in preorder. The node at index N has id N + 1; `parent`
/// is that id, 0 for a root. References with page 0 have an unresolved page
/// in the source structure.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfStructureNode {
    pub parent: u32,
    pub role: PdfBytes,
    pub alt_text: PdfBytes,
    pub actual_text: PdfBytes,
    pub language: PdfBytes,
    pub references: PdfContentReferences,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfContentReference {
    pub page: u32,
    pub mcid: i64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRectangle {
    pub page: u32,
    pub bounds: PdfBox,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfSegment {
    pub page: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPageInfo {
    pub page: u32,
    pub width: f32,
    pub height: f32,
    pub rotation: u32,
}
/// Caller-owned positioned content for `pdf_inspector_compose_items`.
/// Every referenced page needs an entry in `pages`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfComposeInput {
    pub pages: PdfPageInfos,
    pub items: PdfItems,
    pub rectangles: PdfRectangles,
    pub lines: PdfSegments,
    pub structure: PdfStructureElements,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfImage {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: u32,
    pub pixels: PdfBytes,
    pub pixel_to_page: PdfTransform,
    pub page_to_pixel: PdfTransform,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfProvenance {
    pub source: u32,
    pub flags: u32,
    pub confidence: f32,
    pub render_dpi: f32,
    pub render_ms: u64,
    pub ocr_ms: u64,
    pub assembly_ms: u64,
    pub model: PdfBytes,
    pub model_revision: PdfBytes,
    pub warnings: PdfStrings,
}
/// Native-layer text quality numbers. `density` is alphanumeric/visible
/// (`0` when there are no visible characters). `english_cosine` is 1 when
/// there are no ASCII letters. `score` is the 0–1 native-candidate formula.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPageQuality {
    pub alphanumeric_chars: u32,
    pub visible_chars: u32,
    pub density: f32,
    pub replacement_chars: u32,
    pub longest_replacement_run: u32,
    pub english_cosine: f32,
    pub score: f32,
}
/// Horizontal column interval in the request frame; y is not invented.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfInterval {
    pub x0: f32,
    pub x1: f32,
}
/// Load-time repairs recorded at open. `leading_bytes` is the `%PDF-` offset;
/// `saturated_bbox_numerals` counts `/BBox` numerals repaired before parsing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfLoadAudit {
    pub flags: u32,
    pub leading_bytes: u32,
    pub widened_form_bboxes: u32,
    pub saturated_bbox_numerals: u32,
}
/// ToUnicode/CMap coverage observed while decoding positioned content.
/// `interpolated` codes were recovered from a one-code gap; `unmapped`
/// codes could not be decoded.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfCMapGap {
    pub font: PdfBytes,
    pub codes: u32,
    pub interpolated: u32,
    pub unmapped: u32,
}
/// `reading_order` is `PDF_READING_*`; `text_orientation` is
/// `PDF_ORIENTATION_*`, assessed whenever positioned content is parsed
/// (items, text, geometry, or tables). `quality` is
/// filled when native text or items are produced. `columns` are x-only
/// intervals in the request frame. `charts` and `image_regions` are
/// supplemental boxes in that frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPage {
    pub info: PdfPageInfo,
    pub flags: u32,
    pub reading_order: u32,
    pub text_orientation: u32,
    pub quality: PdfPageQuality,
    pub markdown: PdfBytes,
    pub text: PdfBytes,
    pub ocr_reasons: PdfStrings,
    pub items: PdfItems,
    pub columns: PdfIntervals,
    pub charts: PdfBoxes,
    pub image_regions: PdfBoxes,
    pub structure: PdfStructureElements,
    pub rectangles: PdfRectangles,
    pub lines: PdfSegments,
    pub image: PdfImage,
    pub provenance: PdfProvenance,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfCell {
    pub row: u32,
    pub column: u32,
    pub row_span: u32,
    pub column_span: u32,
    pub flags: u32,
    pub bounds: PdfBox,
    pub text: PdfBytes,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRegion {
    pub page: u32,
    pub kind: u32,
    pub flags: u32,
    pub bounds: PdfBox,
    pub text: PdfBytes,
    pub ocr_reason: PdfBytes,
    pub tokens: PdfStrings,
    pub cells: PdfBoxes,
}
/// `kind` is `PDF_TABLE_DATA` or `PDF_TABLE_TOC`. `column_edges` / `row_edges`
/// are detector bands in the request frame when `PDF_TABLE_HAS_BOUNDS` is set.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfTable {
    pub page: u32,
    pub flags: u32,
    /// Index in request.tables for hinted results; meaningful only with PDF_TABLE_FROM_HINT.
    pub input_index: u32,
    pub kind: u32,
    pub bounds: PdfBox,
    pub markdown: PdfBytes,
    pub fallback_reason: PdfBytes,
    pub column_edges: PdfFloats,
    pub row_edges: PdfFloats,
    pub cells: PdfCells,
}
/// One immutable graph of records, published by pointer and released only
/// with pdf_inspector_result_free. Nested storage lasts as long as the
/// result, independently of the source document. `flags` are `PDF_DOC_*`;
/// `pages_sampled` and `pages_with_text` come from detector inspection;
/// `cmap_gaps` is populated when positioned content is parsed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfResult {
    pub present: u32,
    pub pdf_type: u32,
    pub page_count: u32,
    pub confidence: f32,
    pub flags: u32,
    pub pages_sampled: u32,
    pub pages_with_text: u32,
    pub processing_ms: u64,
    pub title: PdfBytes,
    pub markdown: PdfBytes,
    pub text: PdfBytes,
    pub pages: PdfPages,
    pub regions: PdfRegions,
    pub tables: PdfTables,
    pub structure_nodes: PdfStructureNodes,
    pub cmap_gaps: PdfCMapGaps,
}
/// Published by pointer for a failed call; release with pdf_inspector_error_free.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfError {
    pub status: i32,
    pub message: PdfBytes,
}
/// Facts recorded at open, borrowed from the document until
/// pdf_inspector_document_free. `pages` are sheet-frame dimensions of every page.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfDocumentInfo {
    pub page_count: u32,
    pub audit: PdfLoadAudit,
    pub pages: PdfPageInfos,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPageNumbers {
    pub ptr: *const u32,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfStrings {
    pub ptr: *const PdfBytes,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfQuads {
    pub ptr: *const PdfQuad,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfBoxes {
    pub ptr: *const PdfBox,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfIntervals {
    pub ptr: *const PdfInterval,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfFloats {
    pub ptr: *const f32,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfItems {
    pub ptr: *const PdfItem,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPageInfos {
    pub ptr: *const PdfPageInfo,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfStructureElements {
    pub ptr: *const PdfStructureElement,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRectangles {
    pub ptr: *const PdfRectangle,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfSegments {
    pub ptr: *const PdfSegment,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRegionInputs {
    pub ptr: *const PdfRegionInput,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfTableInputs {
    pub ptr: *const PdfTableInput,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOcrSpans {
    pub ptr: *const PdfOcrSpan,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOcrPageInputs {
    pub ptr: *const PdfOcrPageInput,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfPages {
    pub ptr: *const PdfPage,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfRegions {
    pub ptr: *const PdfRegion,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfTables {
    pub ptr: *const PdfTable,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfCells {
    pub ptr: *const PdfCell,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfStructureNodes {
    pub ptr: *const PdfStructureNode,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfCMapGaps {
    pub ptr: *const PdfCMapGap,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PdfContentReferences {
    pub ptr: *const PdfContentReference,
    pub len: usize,
}
