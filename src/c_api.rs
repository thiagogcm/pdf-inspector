#![allow(clippy::missing_safety_doc)]

//! C FFI bindings for pdf-inspector.
//!
//! # Ownership and safety
//!
//! The same four rules hold for every function in this module, so they are
//! stated once here instead of on each entry point:
//!
//! - **Handles are opaque and owned by the caller.** Each handle is released by
//!   its matching `*_free`, exactly once. Passing a handle to any other `*_free`
//!   is undefined behaviour.
//! - **Returned [`CByteView`] / [`CU32View`] pointers are borrowed**, never
//!   freed by the caller, and valid only until the owning handle is freed.
//!   String bytes are UTF-8 and *not* NUL-terminated. A NULL view pointer means
//!   the value is absent; a present-but-empty value has a non-NULL pointer and
//!   zero length.
//! - **Getters are total.** A NULL handle or an out-of-range index yields the
//!   type's zero value (`0`, `0.0`, `false`, NULL, or an `Unknown` enum), never
//!   a panic — they return no [`PdfInspectorError`] and carry no panic guard.
//!   Entry points that allocate report failure through [`PdfInspectorError`]
//!   and zero their out-parameter unconditionally, before any other
//!   validation can fail.
//! - **Handles are not synchronised.** Concurrent reads of one handle are safe;
//!   any call that mutates or frees a handle must not race with another use of
//!   the same handle.
//!
//! Panics at any entry point returning [`PdfInspectorError`] collapse into
//! [`PdfInspectorError::Panic`] rather than unwinding across the FFI boundary.
//!
//! # Error diagnostics
//!
//! [`pdf_inspector_last_error_message`] carries the diagnostic text behind the
//! most recent [`PdfInspectorError`]-returning call on the calling thread; see
//! its doc comment for the exact scoping (getters and `*_free` do not touch
//! this state).

use std::cell::RefCell;
use std::ffi::c_char;

/// C ABI major version; bumped only for incompatible changes. See the
/// Versioning section in `docs/c-api.md`.
pub const PDF_INSPECTOR_ABI_VERSION: u32 = 1;

/// C ABI minor version; bumped for additive, backward-compatible changes.
/// Resets to 0 on every major bump. See `docs/c-api.md`.
pub const PDF_INSPECTOR_ABI_MINOR: u32 = 0;

// =========================================================================
// Enums and Error codes
// =========================================================================

// Every enum below carries a trailing `ReservedMax = 2147483647` sentinel;
// see docs/c-api.md's "Enum width" section for why, and
// tests/c_consumer.c for the regression test.

/// C-compatible error codes returned by the FFI functions.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PdfInspectorError {
    Success = 0,
    IoError = 1,
    ParseError = 2,
    Encrypted = 3,
    InvalidStructure = 4,
    NotAPdf = 5,
    NullPointer = 6,
    Panic = 7,
    InvalidUtf8 = 8,
    InvalidArgument = 9,
    /// Not a real error code; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// FFI-safe representation of PdfType.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CPdfType {
    Unknown = -1,
    TextBased = 0,
    Scanned = 1,
    ImageBased = 2,
    Mixed = 3,
    /// Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// Processing modes accepted by `pdf_inspector_options_set_mode`.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CProcessMode {
    DetectOnly = 0,
    Analyze = 1,
    Full = 2,
    /// Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// Markdown profiles accepted by `pdf_inspector_options_set_profile`.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CMarkdownProfile {
    Fidelity = 0,
    Compact = 1,
    /// Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// Type of an item returned by positioned-text extraction.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CTextItemType {
    Unknown = -1,
    Text = 0,
    Image = 1,
    Link = 2,
    FormField = 3,
    /// Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// Scan strategies accepted by `pdf_inspector_options_set_scan_strategy`.
/// `Sample`/`Pages` carry data that doesn't fit the discriminant, so the
/// setter takes it via separate `sample_size`/`pages` parameters instead.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CScanStrategy {
    /// Scan all pages, stop at the first non-text page.
    EarlyExit = 0,
    /// Scan all pages, no early exit.
    Full = 1,
    /// Sample up to `sample_size` evenly distributed pages.
    Sample = 2,
    /// Only scan the 1-indexed pages listed in `pages`/`pages_count`.
    Pages = 3,
    /// Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
    ReservedMax = 2147483647,
}

/// Borrowed UTF-8 byte view supplied to or returned by the API. The bytes are
/// not NUL-terminated. Returned bytes must not be freed by the caller.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CByteView {
    pub ptr: *const u8,
    pub len: usize,
}

impl Default for CByteView {
    fn default() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
}

/// Borrowed `u32` slice returned by array getters. The elements must not be
/// freed by the caller.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CU32View {
    pub ptr: *const u32,
    pub len: usize,
}

impl Default for CU32View {
    fn default() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }
}

fn map_error(err: crate::PdfError) -> PdfInspectorError {
    // Only `Parse`/`NotAPdf` carry diagnostic text; other variants clear the
    // slot so a stale message doesn't stick to an unrelated error.
    let (message, code) = match err {
        crate::PdfError::Io(_) => (None, PdfInspectorError::IoError),
        crate::PdfError::Parse(msg) => (Some(msg), PdfInspectorError::ParseError),
        crate::PdfError::Encrypted => (None, PdfInspectorError::Encrypted),
        crate::PdfError::InvalidStructure => (None, PdfInspectorError::InvalidStructure),
        crate::PdfError::NotAPdf(msg) => (Some(msg), PdfInspectorError::NotAPdf),
    };
    set_last_error(message);
    code
}

// =========================================================================
// Last-error diagnostics
// =========================================================================

thread_local! {
    // Belongs to the calling thread's most recent entry-point call; see the
    // module-level "Error diagnostics" section.
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn clear_last_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

fn set_last_error(message: Option<String>) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message);
}

fn c_pdf_type(pdf_type: crate::PdfType) -> CPdfType {
    match pdf_type {
        crate::PdfType::TextBased => CPdfType::TextBased,
        crate::PdfType::Scanned => CPdfType::Scanned,
        crate::PdfType::ImageBased => CPdfType::ImageBased,
        crate::PdfType::Mixed => CPdfType::Mixed,
    }
}

// Setters take an `i32`, not the C enum directly: a `#[repr(C)]` enum with a
// discriminant outside its variant list is UB, and C callers can pass any
// int. These exhaustive mappings turn a new Rust variant into a compile
// error here rather than one silently unreachable from C.

fn c_process_mode(mode: &crate::ProcessMode) -> CProcessMode {
    match mode {
        crate::ProcessMode::DetectOnly => CProcessMode::DetectOnly,
        crate::ProcessMode::Analyze => CProcessMode::Analyze,
        crate::ProcessMode::Full => CProcessMode::Full,
    }
}

fn c_markdown_profile(profile: &crate::MarkdownProfile) -> CMarkdownProfile {
    match profile {
        crate::MarkdownProfile::Fidelity => CMarkdownProfile::Fidelity,
        crate::MarkdownProfile::Compact => CMarkdownProfile::Compact,
    }
}

// `Sample`/`Pages` carry data, so the setter calls this with a representative
// instance of each variant rather than iterating a list of them.
fn c_scan_strategy(strategy: &crate::ScanStrategy) -> CScanStrategy {
    match strategy {
        crate::ScanStrategy::EarlyExit => CScanStrategy::EarlyExit,
        crate::ScanStrategy::Full => CScanStrategy::Full,
        crate::ScanStrategy::Sample(_) => CScanStrategy::Sample,
        crate::ScanStrategy::Pages(_) => CScanStrategy::Pages,
    }
}

fn process_mode_from_i32(value: i32) -> Option<crate::ProcessMode> {
    [
        crate::ProcessMode::DetectOnly,
        crate::ProcessMode::Analyze,
        crate::ProcessMode::Full,
    ]
    .into_iter()
    .find(|mode| c_process_mode(mode) as i32 == value)
}

fn markdown_profile_from_i32(value: i32) -> Option<crate::MarkdownProfile> {
    [
        crate::MarkdownProfile::Fidelity,
        crate::MarkdownProfile::Compact,
    ]
    .into_iter()
    .find(|profile| c_markdown_profile(profile) as i32 == value)
}

// =========================================================================
// Opaque wrappers around Rust types
// =========================================================================

/// Rectangle supplied to region extraction, in PDF points with a top-left
/// origin. The two corners may be supplied in either order.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct CRegion {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// Regions to extract from one 1-indexed page. `regions` may be NULL only
/// when `regions_count` is zero.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CPageRegions {
    pub page: u32,
    pub regions: *const CRegion,
    pub regions_count: usize,
}

/// `CTextItemDescriptor.flags` bits. Unknown bits are rejected with
/// `PdfInspectorError_InvalidArgument`.
pub const PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD: u32 = 1 << 0;
pub const PDF_INSPECTOR_TEXT_ITEM_FLAG_ITALIC: u32 = 1 << 1;
pub const PDF_INSPECTOR_TEXT_ITEM_FLAG_UNDERLINE: u32 = 1 << 2;
pub const PDF_INSPECTOR_TEXT_ITEM_FLAG_STRIKEOUT: u32 = 1 << 3;
/// When set, `CTextItemDescriptor.mcid` carries a marked-content ID; when
/// clear, `mcid` is ignored and the item has none.
pub const PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID: u32 = 1 << 4;

const TEXT_ITEM_FLAGS_ALL: u32 = PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD
    | PDF_INSPECTOR_TEXT_ITEM_FLAG_ITALIC
    | PDF_INSPECTOR_TEXT_ITEM_FLAG_UNDERLINE
    | PDF_INSPECTOR_TEXT_ITEM_FLAG_STRIKEOUT
    | PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID;

/// One caller-supplied positioned text item for
/// `pdf_inspector_text_items_result_add`. Coordinates are PDF points in the same
/// bottom-left-origin space the positioned-text getters return — not the
/// top-left origin region extraction uses. `page` is 1-indexed. `text`,
/// `font`, and `link_url` are borrowed UTF-8 views, read only for the
/// duration of the call; a NULL view pointer is accepted only with zero
/// length and means empty. `item_type` takes a `CTextItemType` discriminant
/// (`Unknown` is rejected); `link_url` is observed only for
/// `CTextItemType_Link`. `flags` is a bitwise OR of
/// `PDF_INSPECTOR_TEXT_ITEM_FLAG_*` values.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CTextItemDescriptor {
    pub page: u32,
    pub text: CByteView,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font: CByteView,
    pub font_size: f32,
    /// A `CTextItemType` discriminant; plain `int32_t` because a `repr(C)`
    /// enum field holding an out-of-range value is undefined behaviour.
    pub item_type: i32,
    pub link_url: CByteView,
    pub flags: u32,
    pub mcid: i64,
}

/// One caller-supplied PDF `re`-operator rectangle for table detection in
/// `pdf_inspector_to_markdown_from_items`. Coordinates are PDF points in the
/// same bottom-left-origin space as positioned-item coordinates. `page` is
/// 1-indexed.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct CPdfRect {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One detected vector-grid cell rectangle in crop-image pixels with a
/// top-left origin.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct CVectorGridCellBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// One TSR cell rectangle or polygon in crop-image pixels. `coordinates`
/// must contain exactly 4 values (`x1,y1,x2,y2`) or 8 polygon coordinates.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CTsrCellBBox {
    pub coordinates: *const f32,
    pub coordinates_count: usize,
}

/// One table region plus externally supplied table-structure recognition
/// output. `page` is 1-indexed. All arrays are borrowed for the duration of
/// the extraction call and may be NULL only when their count is zero.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CTsrTableInput {
    pub page: u32,
    pub crop_pdf_pt_bbox: CRegion,
    pub render_dpi: f32,
    pub structure_tokens: *const CByteView,
    pub structure_tokens_count: usize,
    pub cell_bboxes: *const CTsrCellBBox,
    pub cell_bboxes_count: usize,
}

/// Fixed metadata for one resolved TSR cell. `page_pt_bbox` uses PDF points
/// with a top-left origin. Cell text is available through a separate getter.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct CTsrStructuredCell {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub is_header: bool,
    pub page_pt_bbox: CRegion,
}

/// FFI wrapper around `PdfProcessResult`.
pub struct CPdfProcessResult {
    inner: crate::PdfProcessResult,
}

/// FFI wrapper around `PdfClassification`. `inner.pages_needing_ocr` is
/// normalised to 1-indexed at construction so the zero-copy getter can hand
/// back a borrowed slice.
pub struct CPdfClassification {
    inner: crate::PdfClassification,
}

/// FFI wrapper around the full detector's `PdfTypeResult`. OCR reasons are
/// normalised from an ordered map to a vector for stable indexed traversal.
pub struct CPdfTypeResult {
    inner: crate::PdfTypeResult,
    ocr_reasons_by_page: Vec<crate::PageOcrReasons>,
}

/// FFI wrapper around `PagesExtractionResult`.
pub struct CPagesExtractionResult {
    inner: crate::PagesExtractionResult,
}

/// FFI wrapper around extracted plain text.
pub struct CTextResult {
    text: String,
}

/// FFI wrapper around positioned text items.
pub struct CTextItemsResult {
    items: Vec<crate::TextItem>,
}

/// FFI wrapper around tagged-PDF structure-tree elements.
pub struct CStructureElementsResult {
    elements: Vec<crate::StructureElement>,
}

/// FFI wrapper around region-based text extraction results.
pub struct CRegionTextResult {
    pages: Vec<crate::PageRegionResult>,
}

/// FFI wrapper around an optional `VectorGridDetection`. A successful call
/// always returns a handle; use `pdf_inspector_vector_grid_result_is_detected`
/// to distinguish a detected grid from a valid no-grid result.
pub struct CVectorGridResult {
    detection: Option<crate::VectorGridDetection>,
}

/// FFI wrapper around auto-fallback TSR table extraction results.
pub struct CTsrTableExtractionResult {
    results: Vec<crate::TableExtractionResult>,
}

/// FFI wrapper around raw TSR structured-cell results, one cell list per
/// input descriptor in input order.
pub struct CTsrStructuredCellsResult {
    tables: Vec<Vec<crate::tables::StructuredCell>>,
}

type RegionRequests = Vec<(u32, Vec<[f32; 4]>)>;

// =========================================================================
// Panic guards, input conversion, and borrowed-value helpers
// =========================================================================

/// Run `f` under a panic guard, collapsing its `Result` into a C error code.
/// Every `PdfInspectorError`-returning entry point funnels through here
/// (directly, or via [`with_options`]), so clearing the last-error slot here
/// clears it for all of them.
fn catch_panic_err<F>(f: F) -> PdfInspectorError
where
    F: FnOnce() -> Result<(), PdfInspectorError>,
{
    clear_last_error();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => PdfInspectorError::Success,
        Ok(Err(err)) => err,
        Err(payload) => {
            set_last_error(panic_message(&payload));
            PdfInspectorError::Panic
        }
    }
}

/// Best-effort message from a caught panic payload (`&str`/`String`, the two
/// shapes `panic!`/`.unwrap()`/`.expect()` produce; anything else has none).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> Option<String> {
    if let Some(s) = payload.downcast_ref::<&str>() {
        Some((*s).to_string())
    } else {
        payload.downcast_ref::<String>().cloned()
    }
}

/// Read a PDF file for a path-based entry point, mapping I/O failures onto
/// the shared error mapping.
fn read_pdf_file(path: &str) -> Result<Vec<u8>, PdfInspectorError> {
    std::fs::read(path).map_err(|error| map_error(error.into()))
}

/// Drop a `Box`-allocated handle, tolerating NULL and swallowing panics.
unsafe fn free_boxed<T>(ptr: *mut T) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !ptr.is_null() {
            let _ = Box::from_raw(ptr);
        }
    }));
}

/// Mutate `options` under a panic guard, rejecting a NULL handle.
unsafe fn with_options(
    options: *mut crate::PdfOptions,
    f: impl FnOnce(&mut crate::PdfOptions) -> Result<(), PdfInspectorError>,
) -> PdfInspectorError {
    catch_panic_err(|| {
        let Some(opts) = options.as_mut() else {
            return Err(PdfInspectorError::NullPointer);
        };
        f(opts)
    })
}

/// `with_options` for setters that cannot fail once the handle is valid.
unsafe fn set_option(
    options: *mut crate::PdfOptions,
    f: impl FnOnce(&mut crate::PdfOptions),
) -> PdfInspectorError {
    with_options(options, |opts| {
        f(opts);
        Ok(())
    })
}

/// Publish a freshly built handle through an out-parameter, under the panic
/// guard every allocating entry point shares. `result_out` is zeroed before
/// `build` runs, so the module-level "zero the out-parameter before any other
/// validation can fail" rule holds in one place instead of at each call site.
unsafe fn emit_handle<T>(
    result_out: *mut *mut T,
    build: impl FnOnce() -> Result<T, PdfInspectorError>,
) -> PdfInspectorError {
    catch_panic_err(|| {
        let Some(slot) = result_out.as_mut() else {
            return Err(PdfInspectorError::NullPointer);
        };
        *slot = std::ptr::null_mut();
        *slot = Box::into_raw(Box::new(build()?));
        Ok(())
    })
}

unsafe fn options_or_default(options: *const crate::PdfOptions) -> crate::PdfOptions {
    options.as_ref().cloned().unwrap_or_default()
}

unsafe fn markdown_options_or_default(options: *const crate::PdfOptions) -> crate::MarkdownOptions {
    options
        .as_ref()
        .map(|options| options.markdown.clone())
        .unwrap_or_default()
}

unsafe fn detection_options_or_default(
    options: *const crate::PdfOptions,
) -> (crate::detector::DetectionConfig, Option<String>) {
    options.as_ref().map_or_else(Default::default, |options| {
        (options.detection.clone(), options.password.clone())
    })
}

unsafe fn str_from_ffi<'a>(ptr: *const c_char) -> Result<&'a str, PdfInspectorError> {
    if ptr.is_null() {
        return Err(PdfInspectorError::NullPointer);
    }
    std::ffi::CStr::from_ptr(ptr)
        .to_str()
        .map_err(|_| PdfInspectorError::InvalidUtf8)
}

/// Convert an optional password C string. NULL means "no password", the same
/// convention `pdf_inspector_options_set_password` uses to clear one.
unsafe fn password_from_ffi<'a>(
    password: *const c_char,
) -> Result<Option<&'a str>, PdfInspectorError> {
    if password.is_null() {
        Ok(None)
    } else {
        Ok(Some(str_from_ffi(password)?))
    }
}

unsafe fn bytes_from_ffi<'a>(
    buffer: *const u8,
    size: usize,
) -> Result<&'a [u8], PdfInspectorError> {
    if buffer.is_null() {
        return if size == 0 {
            Ok(&[])
        } else {
            Err(PdfInspectorError::NullPointer)
        };
    }
    if size > isize::MAX as usize {
        return Err(PdfInspectorError::InvalidArgument);
    }
    Ok(std::slice::from_raw_parts(buffer, size))
}

unsafe fn descriptor_slice_from_ffi<'a, T>(
    ptr: *const T,
    count: usize,
) -> Result<&'a [T], PdfInspectorError> {
    if ptr.is_null() {
        return if count == 0 {
            Ok(&[])
        } else {
            Err(PdfInspectorError::InvalidArgument)
        };
    }
    let element_size = std::mem::size_of::<T>();
    if element_size != 0 && count > isize::MAX as usize / element_size {
        return Err(PdfInspectorError::InvalidArgument);
    }
    Ok(std::slice::from_raw_parts(ptr, count))
}

/// Every page-list entry point routes through here, which is what makes
/// "page 0 is always rejected" one rule instead of one per call site.
unsafe fn pages_from_ffi<'a>(
    pages: *const u32,
    pages_count: usize,
) -> Result<Option<&'a [u32]>, PdfInspectorError> {
    if pages.is_null() {
        return if pages_count == 0 {
            Ok(None)
        } else {
            Err(PdfInspectorError::InvalidArgument)
        };
    }
    let pages = descriptor_slice_from_ffi(pages, pages_count)?;
    if pages.contains(&0) {
        return Err(PdfInspectorError::InvalidArgument);
    }
    Ok(Some(pages))
}

/// 1-indexed page list to the 0-indexed form `extract_pages_markdown` expects.
unsafe fn pages_zero_indexed_from_ffi(
    pages: *const u32,
    pages_count: usize,
) -> Result<Option<Vec<u32>>, PdfInspectorError> {
    let Some(pages) = pages_from_ffi(pages, pages_count)? else {
        return Ok(None);
    };
    pages
        .iter()
        .map(|page| {
            page.checked_sub(1)
                .ok_or(PdfInspectorError::InvalidArgument)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

unsafe fn pages_set_from_ffi(
    pages: *const u32,
    pages_count: usize,
) -> Result<Option<std::collections::HashSet<u32>>, PdfInspectorError> {
    Ok(pages_from_ffi(pages, pages_count)?.map(|pages| pages.iter().copied().collect()))
}

/// The one definition of "non-finite float input is `InvalidArgument`".
fn require_finite(values: &[f32]) -> Result<(), PdfInspectorError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(PdfInspectorError::InvalidArgument)
    }
}

/// Validate one 1-indexed page number crossing the ABI. Together with
/// `pages_from_ffi` this keeps "page 0 is always rejected" one rule instead
/// of one per call site.
fn page_number_from_ffi(page: u32) -> Result<u32, PdfInspectorError> {
    if page == 0 {
        Err(PdfInspectorError::InvalidArgument)
    } else {
        Ok(page)
    }
}

/// `page_number_from_ffi`, converted to the 0-indexed form the Rust region,
/// vector-grid, and TSR APIs expect.
fn page_index_from_ffi(page: u32) -> Result<u32, PdfInspectorError> {
    page_number_from_ffi(page).map(|page| page - 1)
}

/// Read a `CRegion`'s coordinates, rejecting non-finite values.
fn region_coordinates_from_ffi(region: &CRegion) -> Result<[f32; 4], PdfInspectorError> {
    let coordinates = [region.x1, region.y1, region.x2, region.y2];
    require_finite(&coordinates)?;
    Ok(coordinates)
}

/// Convert the C page/region descriptors to the Rust API's 0-indexed page
/// representation. The returned vectors own their coordinates, so callers
/// may release the input arrays as soon as the entry point returns.
unsafe fn page_regions_from_ffi(
    page_regions: *const CPageRegions,
    page_regions_count: usize,
) -> Result<RegionRequests, PdfInspectorError> {
    descriptor_slice_from_ffi(page_regions, page_regions_count)?
        .iter()
        .map(|page_regions| {
            let page = page_index_from_ffi(page_regions.page)?;
            let regions =
                descriptor_slice_from_ffi(page_regions.regions, page_regions.regions_count)?
                    .iter()
                    .map(region_coordinates_from_ffi)
                    .collect::<Result<Vec<_>, _>>()?;
            Ok((page, regions))
        })
        .collect()
}

/// Borrow a `CByteView`'s bytes as UTF-8. NULL is accepted only with zero
/// length and means empty.
unsafe fn utf8_from_view<'a>(view: &CByteView) -> Result<&'a str, PdfInspectorError> {
    let bytes = bytes_from_ffi(view.ptr, view.len)?;
    std::str::from_utf8(bytes).map_err(|_| PdfInspectorError::InvalidUtf8)
}

/// Map a `CTextItemType` discriminant to the Rust item type, allocating
/// `link_url` only for `Link`. `Unknown` and unlisted values map to `None`.
/// The inverse of the exhaustive `text_item_type` mapping.
fn item_type_from_i32(value: i32, link_url: &str) -> Option<crate::types::ItemType> {
    Some(match value {
        v if v == CTextItemType::Text as i32 => crate::types::ItemType::Text,
        v if v == CTextItemType::Image as i32 => crate::types::ItemType::Image,
        v if v == CTextItemType::Link as i32 => crate::types::ItemType::Link(link_url.to_owned()),
        v if v == CTextItemType::FormField as i32 => crate::types::ItemType::FormField,
        _ => return None,
    })
}

/// Convert caller-supplied item descriptors, owning every string so the
/// input arrays may be released as soon as the entry point returns. Fails
/// without partial output, which is what makes
/// `pdf_inspector_text_items_result_add` atomic.
unsafe fn text_items_from_ffi(
    descriptors: *const CTextItemDescriptor,
    descriptors_count: usize,
) -> Result<Vec<crate::TextItem>, PdfInspectorError> {
    descriptor_slice_from_ffi(descriptors, descriptors_count)?
        .iter()
        .map(|descriptor| {
            let page = page_number_from_ffi(descriptor.page)?;
            if descriptor.flags & !TEXT_ITEM_FLAGS_ALL != 0 {
                return Err(PdfInspectorError::InvalidArgument);
            }
            require_finite(&[
                descriptor.x,
                descriptor.y,
                descriptor.width,
                descriptor.height,
                descriptor.font_size,
            ])?;
            let text = utf8_from_view(&descriptor.text)?.to_owned();
            let font = utf8_from_view(&descriptor.font)?.to_owned();
            let item_type =
                item_type_from_i32(descriptor.item_type, utf8_from_view(&descriptor.link_url)?)
                    .ok_or(PdfInspectorError::InvalidArgument)?;
            Ok(crate::TextItem {
                text,
                x: descriptor.x,
                y: descriptor.y,
                width: descriptor.width,
                height: descriptor.height,
                font,
                font_size: descriptor.font_size,
                page,
                is_bold: descriptor.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD != 0,
                is_italic: descriptor.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_ITALIC != 0,
                is_underline: descriptor.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_UNDERLINE != 0,
                is_strikeout: descriptor.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_STRIKEOUT != 0,
                item_type,
                mcid: (descriptor.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID != 0)
                    .then_some(descriptor.mcid),
            })
        })
        .collect()
}

/// Convert caller-supplied `re`-operator rectangles for table detection. The
/// returned vector owns its data, so the input array may be released as soon
/// as the entry point returns. Page 0 and non-finite coordinates are
/// `InvalidArgument`.
unsafe fn pdf_rects_from_ffi(
    rects: *const CPdfRect,
    rects_count: usize,
) -> Result<Vec<crate::PdfRect>, PdfInspectorError> {
    descriptor_slice_from_ffi(rects, rects_count)?
        .iter()
        .map(|rect| {
            let page = page_number_from_ffi(rect.page)?;
            require_finite(&[rect.x, rect.y, rect.width, rect.height])?;
            Ok(crate::PdfRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                page,
            })
        })
        .collect()
}

unsafe fn vector_grid_request_from_ffi(
    page: u32,
    region: *const CRegion,
    render_dpi: f32,
) -> Result<(u32, [f32; 4]), PdfInspectorError> {
    let page = page_index_from_ffi(page)?;
    let Some(region) = region.as_ref() else {
        return Err(PdfInspectorError::NullPointer);
    };
    let [x1, y1, x2, y2] = region_coordinates_from_ffi(region)?;
    if !render_dpi.is_finite() || render_dpi <= 0.0 {
        return Err(PdfInspectorError::InvalidArgument);
    }
    let coordinates = [x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)];
    let ppi = crate::ppi_for_render_dpi(render_dpi);
    let crop_width = (coordinates[2] - coordinates[0]) * ppi;
    let crop_height = (coordinates[3] - coordinates[1]) * ppi;
    if !crop_width.is_finite() || !crop_height.is_finite() {
        return Err(PdfInspectorError::InvalidArgument);
    }
    Ok((page, coordinates))
}

unsafe fn tsr_inputs_from_ffi(
    inputs: *const CTsrTableInput,
    inputs_count: usize,
) -> Result<Vec<crate::TsrTableInput>, PdfInspectorError> {
    descriptor_slice_from_ffi(inputs, inputs_count)?
        .iter()
        .map(|input| {
            let page = page_index_from_ffi(input.page)?;
            let crop = region_coordinates_from_ffi(&input.crop_pdf_pt_bbox)?;
            if crop[0] >= crop[2]
                || crop[1] >= crop[3]
                || !input.render_dpi.is_finite()
                || input.render_dpi <= 0.0
            {
                return Err(PdfInspectorError::InvalidArgument);
            }
            let ppi = crate::ppi_for_render_dpi(input.render_dpi);
            if !((crop[2] - crop[0]) * ppi).is_finite() || !((crop[3] - crop[1]) * ppi).is_finite()
            {
                return Err(PdfInspectorError::InvalidArgument);
            }

            let structure_tokens =
                descriptor_slice_from_ffi(input.structure_tokens, input.structure_tokens_count)?
                    .iter()
                    .map(|token| utf8_from_view(token).map(str::to_owned))
                    .collect::<Result<Vec<_>, _>>()?;

            let cell_bboxes =
                descriptor_slice_from_ffi(input.cell_bboxes, input.cell_bboxes_count)?
                    .iter()
                    .map(|bbox| {
                        if bbox.coordinates_count != 4 && bbox.coordinates_count != 8 {
                            return Err(PdfInspectorError::InvalidArgument);
                        }
                        let coordinates =
                            descriptor_slice_from_ffi(bbox.coordinates, bbox.coordinates_count)?;
                        require_finite(coordinates)?;
                        let points = coordinates.as_chunks::<2>().0;
                        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
                        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                        for point in points {
                            min_x = min_x.min(point[0]);
                            min_y = min_y.min(point[1]);
                            max_x = max_x.max(point[0]);
                            max_y = max_y.max(point[1]);
                        }
                        if min_x >= max_x
                            || min_y >= max_y
                            || !points.iter().all(|point| {
                                (crop[0] + point[0] / ppi).is_finite()
                                    && (crop[1] + point[1] / ppi).is_finite()
                            })
                        {
                            return Err(PdfInspectorError::InvalidArgument);
                        }
                        Ok(coordinates.to_vec())
                    })
                    .collect::<Result<Vec<_>, _>>()?;

            let parsed_cell_count =
                crate::tables::structured::parse_structure_checked(&structure_tokens)
                    .ok_or(PdfInspectorError::InvalidArgument)?
                    .len();
            if parsed_cell_count != cell_bboxes.len() {
                return Err(PdfInspectorError::InvalidArgument);
            }

            Ok(crate::TsrTableInput {
                page,
                crop_pdf_pt_bbox: crop,
                render_dpi: input.render_dpi,
                structure_tokens,
                cell_bboxes,
            })
        })
        .collect()
}

/// Write a borrowed UTF-8 view, zeroing `out` first. Returns `false` when
/// `out` is NULL or `value` is absent. Empty present strings retain their
/// non-NULL slice pointer with a zero length.
unsafe fn byte_view_out(value: Option<&str>, out: *mut CByteView) -> bool {
    let Some(out) = out.as_mut() else {
        return false;
    };
    *out = CByteView::default();
    let Some(value) = value else {
        return false;
    };
    out.ptr = value.as_ptr();
    out.len = value.len();
    true
}

/// `byte_view_out` for borrowed `u32` slices.
unsafe fn u32_view_out(values: Option<&[u32]>, out: *mut CU32View) -> bool {
    let Some(out) = out.as_mut() else {
        return false;
    };
    *out = CU32View::default();
    let Some(values) = values else {
        return false;
    };
    out.ptr = values.as_ptr();
    out.len = values.len();
    true
}

fn text_item_type(item_type: &crate::types::ItemType) -> CTextItemType {
    match item_type {
        crate::types::ItemType::Text => CTextItemType::Text,
        crate::types::ItemType::Image => CTextItemType::Image,
        crate::types::ItemType::Link(_) => CTextItemType::Link,
        crate::types::ItemType::FormField => CTextItemType::FormField,
    }
}

fn text_item_link_url(item_type: &crate::types::ItemType) -> Option<&String> {
    match item_type {
        crate::types::ItemType::Link(url) => Some(url),
        _ => None,
    }
}

// Borrow-or-default accessors backing the getter families below. Each returns
// `default` for a NULL handle or an out-of-range index.

unsafe fn with_result<R>(
    result: *const CPdfProcessResult,
    default: R,
    f: impl FnOnce(&crate::PdfProcessResult) -> R,
) -> R {
    result.as_ref().map_or(default, |result| f(&result.inner))
}

unsafe fn with_classification<R>(
    classification: *const CPdfClassification,
    default: R,
    f: impl FnOnce(&crate::PdfClassification) -> R,
) -> R {
    classification
        .as_ref()
        .map_or(default, |classification| f(&classification.inner))
}

unsafe fn with_pdf_type_result<R>(
    result: *const CPdfTypeResult,
    default: R,
    f: impl FnOnce(&CPdfTypeResult) -> R,
) -> R {
    result.as_ref().map_or(default, f)
}

fn c_full_detection_result(mut inner: crate::PdfTypeResult) -> CPdfTypeResult {
    let ocr_reasons_by_page =
        crate::page_ocr_reasons_vec(std::mem::take(&mut inner.ocr_reasons_by_page));
    CPdfTypeResult {
        inner,
        ocr_reasons_by_page,
    }
}

unsafe fn with_pages_result<R>(
    result: *const CPagesExtractionResult,
    default: R,
    f: impl FnOnce(&crate::PagesExtractionResult) -> R,
) -> R {
    result.as_ref().map_or(default, |result| f(&result.inner))
}

unsafe fn with_page<R>(
    result: *const CPagesExtractionResult,
    index: usize,
    default: R,
    f: impl FnOnce(&crate::PageMarkdown) -> R,
) -> R {
    result
        .as_ref()
        .and_then(|result| result.inner.pages.get(index))
        .map_or(default, f)
}

unsafe fn with_text_item<R>(
    result: *const CTextItemsResult,
    index: usize,
    default: R,
    f: impl FnOnce(&crate::TextItem) -> R,
) -> R {
    result
        .as_ref()
        .and_then(|result| result.items.get(index))
        .map_or(default, f)
}

unsafe fn with_structure_element<R>(
    result: *const CStructureElementsResult,
    index: usize,
    default: R,
    f: impl FnOnce(&crate::StructureElement) -> R,
) -> R {
    result
        .as_ref()
        .and_then(|result| result.elements.get(index))
        .map_or(default, f)
}

unsafe fn with_region_page<R>(
    result: *const CRegionTextResult,
    page_index: usize,
    default: R,
    f: impl FnOnce(&crate::PageRegionResult) -> R,
) -> R {
    result
        .as_ref()
        .and_then(|result| result.pages.get(page_index))
        .map_or(default, f)
}

unsafe fn with_region<R>(
    result: *const CRegionTextResult,
    page_index: usize,
    region_index: usize,
    default: R,
    f: impl FnOnce(&crate::RegionText) -> R,
) -> R {
    result
        .as_ref()
        .and_then(|result| result.pages.get(page_index))
        .and_then(|page| page.regions.get(region_index))
        .map_or(default, f)
}

// Shared across the two identical OCR-reason getter families
// (`CPdfProcessResult` and `CPagesExtractionResult`).

fn ocr_reasons_page(entries: &[crate::PageOcrReasons], index: usize) -> u32 {
    entries.get(index).map_or(0, |entry| entry.page)
}

fn ocr_reasons_len(entries: &[crate::PageOcrReasons], index: usize) -> usize {
    entries.get(index).map_or(0, |entry| entry.reasons.len())
}

fn ocr_reason_at(
    entries: &[crate::PageOcrReasons],
    index: usize,
    reason_index: usize,
) -> Option<&String> {
    entries
        .get(index)
        .and_then(|entry| entry.reasons.get(reason_index))
}

// =========================================================================
// PdfOptions API
// =========================================================================

/// Return the C ABI major version. See the Versioning section in `docs/c-api.md`.
#[no_mangle]
pub extern "C" fn pdf_inspector_abi_version() -> u32 {
    PDF_INSPECTOR_ABI_VERSION
}

/// Return the C ABI minor version. See the Versioning section in `docs/c-api.md`.
#[no_mangle]
pub extern "C" fn pdf_inspector_abi_minor() -> u32 {
    PDF_INSPECTOR_ABI_MINOR
}

/// Get the UTF-8 diagnostic message behind the most recent
/// `PdfInspectorError`-returning call on the calling thread. Returns `false`
/// and zeroes `out` if that call succeeded or left no diagnostic text. Getters
/// and `*_free` never touch this slot. The view stays valid until the next
/// fallible entry-point call on this thread.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_last_error_message(out: *mut CByteView) -> bool {
    LAST_ERROR.with(|slot| byte_view_out(slot.borrow().as_deref(), out))
}

/// Create a new `PdfOptions` instance with default settings.
/// Must be freed with `pdf_inspector_options_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_new() -> *mut crate::PdfOptions {
    std::panic::catch_unwind(|| Box::into_raw(Box::new(crate::PdfOptions::default())))
        .unwrap_or(std::ptr::null_mut())
}

/// Free a `PdfOptions` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_free(options: *mut crate::PdfOptions) {
    free_boxed(options);
}

/// Set the processing mode to a `CProcessMode` value.
/// Out-of-range values are rejected with `InvalidArgument`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_mode(
    options: *mut crate::PdfOptions,
    mode: i32,
) -> PdfInspectorError {
    with_options(options, |opts| {
        let Some(mode) = process_mode_from_i32(mode) else {
            return Err(PdfInspectorError::InvalidArgument);
        };
        opts.mode = mode;
        Ok(())
    })
}

/// Set the password for decrypting an encrypted PDF.
/// Pass NULL to clear the password.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_password(
    options: *mut crate::PdfOptions,
    password: *const c_char,
) -> PdfInspectorError {
    with_options(options, |opts| {
        opts.password = password_from_ffi(password)?.map(str::to_string);
        Ok(())
    })
}

/// Limit processing to specific 1-indexed page. Can be called multiple times.
/// Page 0 has no 1-indexed meaning and is rejected with `InvalidArgument`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_add_page(
    options: *mut crate::PdfOptions,
    page: u32,
) -> PdfInspectorError {
    with_options(options, |opts| {
        if page == 0 {
            return Err(PdfInspectorError::InvalidArgument);
        }
        opts.page_filter
            .get_or_insert_with(Default::default)
            .insert(page);
        Ok(())
    })
}

/// Clear the page filter, restoring processing of every page.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_clear_pages(
    options: *mut crate::PdfOptions,
) -> PdfInspectorError {
    set_option(options, |opts| opts.page_filter = None)
}

/// Set whether to detect headers by font size.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_headers(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_headers = enable)
}

/// Set whether to detect list items.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_lists(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_lists = enable)
}

/// Set whether to detect code blocks.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_code(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_code = enable)
}

/// Set whether to remove standalone page numbers.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_remove_page_numbers(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.remove_page_numbers = enable)
}

/// Set whether to convert URLs to markdown links.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_format_urls(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.format_urls = enable)
}

/// Set whether to fix hyphenation (broken words across lines).
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_fix_hyphenation(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.fix_hyphenation = enable)
}

/// Set whether to detect and format bold text.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_bold(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_bold = enable)
}

/// Set whether to detect and format italic text.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_italic(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_italic = enable)
}

/// Set whether to emit `<u>` runs for text with an underline.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_detect_underline(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.detect_underline = enable)
}

/// Set whether to include image placeholders in output.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_include_images(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.include_images = enable)
}

/// Set whether to include extracted hyperlinks.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_include_links(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.include_links = enable)
}

/// Set whether to insert page break markers (<!-- Page N -->) between pages.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_include_page_numbers(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.include_page_numbers = enable)
}

/// Set whether to strip repeated headers/footers.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_strip_headers_footers(
    options: *mut crate::PdfOptions,
    enable: bool,
) -> PdfInspectorError {
    set_option(options, |opts| opts.markdown.strip_headers_footers = enable)
}

/// Set the markdown profile to a `CMarkdownProfile` value.
/// Out-of-range values are rejected with `InvalidArgument`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_profile(
    options: *mut crate::PdfOptions,
    profile: i32,
) -> PdfInspectorError {
    with_options(options, |opts| {
        let Some(profile) = markdown_profile_from_i32(profile) else {
            return Err(PdfInspectorError::InvalidArgument);
        };
        opts.markdown.profile = profile;
        Ok(())
    })
}

/// Set minimum text operator count per page to consider as text-based.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_min_text_ops_per_page(
    options: *mut crate::PdfOptions,
    count: u32,
) -> PdfInspectorError {
    set_option(options, |opts| {
        opts.detection.min_text_ops_per_page = count;
    })
}

/// Set threshold ratio of text pages to total pages for classification.
/// Only finite values in the inclusive range `0.0..=1.0` are accepted.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_text_page_ratio_threshold(
    options: *mut crate::PdfOptions,
    threshold: f32,
) -> PdfInspectorError {
    with_options(options, |opts| {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(PdfInspectorError::InvalidArgument);
        }
        opts.detection.text_page_ratio_threshold = threshold;
        Ok(())
    })
}

/// Set the page-detection scan strategy from a `CScanStrategy` discriminant.
/// `sample_size` is used only for `CScanStrategy_Sample` (scan up to this
/// many evenly distributed pages; 0 is rejected). `pages`/`pages_count` are
/// used only for `CScanStrategy_Pages` (1-indexed pages to scan; NULL with a
/// nonzero count, an empty list, or any page number of 0 is rejected). Both
/// are ignored for `CScanStrategy_EarlyExit` and `CScanStrategy_Full`.
/// Out-of-range `strategy` values are rejected with `InvalidArgument`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_scan_strategy(
    options: *mut crate::PdfOptions,
    strategy: i32,
    sample_size: u32,
    pages: *const u32,
    pages_count: usize,
) -> PdfInspectorError {
    with_options(options, |opts| {
        // Compared against `c_scan_strategy` of a representative instance,
        // not the raw `CScanStrategy::X as i32` discriminant directly, so
        // that adding a `ScanStrategy` variant without updating
        // `c_scan_strategy` (and, transitively, this setter) is a compile
        // error rather than a variant silently unreachable from C.
        let strategy = match strategy {
            x if x == c_scan_strategy(&crate::ScanStrategy::EarlyExit) as i32 => {
                crate::ScanStrategy::EarlyExit
            }
            x if x == c_scan_strategy(&crate::ScanStrategy::Full) as i32 => {
                crate::ScanStrategy::Full
            }
            x if x == c_scan_strategy(&crate::ScanStrategy::Sample(sample_size)) as i32 => {
                if sample_size == 0 {
                    return Err(PdfInspectorError::InvalidArgument);
                }
                crate::ScanStrategy::Sample(sample_size)
            }
            x if x == c_scan_strategy(&crate::ScanStrategy::Pages(Vec::new())) as i32 => {
                match pages_from_ffi(pages, pages_count)? {
                    Some(pages) if !pages.is_empty() => crate::ScanStrategy::Pages(pages.to_vec()),
                    _ => return Err(PdfInspectorError::InvalidArgument),
                }
            }
            _ => return Err(PdfInspectorError::InvalidArgument),
        };
        opts.detection.strategy = strategy;
        Ok(())
    })
}

/// Below this, heading-tier detection's `font_size / base_size` ratio
/// explodes toward a near-zero base, making every line look like a
/// top-tier heading — rejects a technically-finite denormal like `1e-45`
/// that `> 0.0` alone would not.
const MIN_BASE_FONT_SIZE: f32 = 1.0;

/// Set the base font size (in points) used as the body-text baseline for
/// header-size comparisons. A finite value `>= 1.0` sets an explicit
/// override; any other finite value (`< 1.0`, including 0 and negatives)
/// clears the override and restores automatic detection from the document's
/// dominant font size, which is also the default. NaN and infinite values
/// are rejected with `InvalidArgument`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_options_set_base_font_size(
    options: *mut crate::PdfOptions,
    size: f32,
) -> PdfInspectorError {
    with_options(options, |opts| {
        if !size.is_finite() {
            return Err(PdfInspectorError::InvalidArgument);
        }
        opts.markdown.base_font_size = (size >= MIN_BASE_FONT_SIZE).then_some(size);
        Ok(())
    })
}

// =========================================================================
// Main Entry Points
// =========================================================================

/// Process a PDF file with options.
/// Returns Success on success and populates `result_out` with an opaque `CPdfProcessResult` pointer.
/// If `options` is NULL, default options are used.
/// The output result must be freed using `pdf_inspector_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_process_pdf(
    path: *const c_char,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CPdfProcessResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let inner = crate::process_pdf_with_options(path, options_or_default(options))
            .map_err(map_error)?;
        Ok(CPdfProcessResult { inner })
    })
}

/// Process PDF bytes with options. A NULL buffer is accepted only when `size` is zero.
/// Returns Success on success and populates `result_out` with an opaque `CPdfProcessResult` pointer.
/// If `options` is NULL, default options are used.
/// The output result must be freed using `pdf_inspector_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_process_pdf_mem(
    buffer: *const u8,
    size: usize,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CPdfProcessResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let inner = crate::process_pdf_mem_with_options(buffer, options_or_default(options))
            .map_err(map_error)?;
        Ok(CPdfProcessResult { inner })
    })
}

/// Run full PDF type detection on a file. If `options` is NULL, defaults are
/// used. Only the detection settings and password are observed; processing,
/// Markdown, and page-filter settings are ignored. Free the returned handle
/// with `pdf_inspector_pdf_type_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_detect_pdf_type(
    path: *const c_char,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CPdfTypeResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let (detection, password) = detection_options_or_default(options);
        let inner = crate::detector::detect_pdf_type_with_config_and_password(
            path,
            detection,
            password.as_deref(),
        )
        .map_err(map_error)?;
        Ok(c_full_detection_result(inner))
    })
}

/// Run full PDF type detection on PDF bytes. A NULL buffer is accepted only
/// when `size` is zero. If `options` is NULL, defaults are used. Only the
/// detection settings and password are observed; processing, Markdown, and
/// page-filter settings are ignored. Free the returned handle with
/// `pdf_inspector_pdf_type_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_detect_pdf_type_mem(
    buffer: *const u8,
    size: usize,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CPdfTypeResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let (detection, password) = detection_options_or_default(options);
        let inner = crate::detector::detect_pdf_type_mem_with_config_and_password(
            buffer,
            detection,
            password.as_deref(),
        )
        .map_err(map_error)?;
        Ok(c_full_detection_result(inner))
    })
}

/// Classify a PDF from a memory buffer without extracting text.
/// A NULL buffer is accepted only when `size` is zero. `password` decrypts an
/// encrypted PDF (NULL = none; see `pdf_inspector_options_set_password`).
/// Returns Success on success and populates `result_out` with an opaque `CPdfClassification` pointer.
/// Must be freed with `pdf_inspector_classification_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classify_pdf_mem(
    buffer: *const u8,
    size: usize,
    password: *const c_char,
    result_out: *mut *mut CPdfClassification,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let password = password_from_ffi(password)?;
        let mut inner =
            crate::classify_pdf_mem_with_password(buffer, password).map_err(map_error)?;
        // Page numbers are 1-indexed throughout this ABI; `PdfClassification`
        // is the one Rust type that reports them 0-indexed.
        for page in &mut inner.pages_needing_ocr {
            *page = page.saturating_add(1);
        }
        Ok(CPdfClassification { inner })
    })
}

/// Convert UTF-8 plain text to basic Markdown. A NULL `text` pointer is
/// accepted only when `size` is zero. If `options` is NULL, defaults are used.
/// Only Markdown settings are observed; processing mode, detection settings,
/// password, and page filters are ignored. The result must be freed with
/// `pdf_inspector_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_to_markdown(
    text: *const u8,
    size: usize,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let text = bytes_from_ffi(text, size)?;
        let text = std::str::from_utf8(text).map_err(|_| PdfInspectorError::InvalidUtf8)?;
        let markdown = crate::to_markdown(text, markdown_options_or_default(options));
        Ok(CTextResult { text: markdown })
    })
}

/// Convert positioned text items to Markdown. `items` is borrowed and remains
/// valid and reusable after the call; it may come from
/// `pdf_inspector_extract_text_with_positions` or be caller-built with
/// `pdf_inspector_text_items_result_new`/`_add`.
///
/// `rects`/`rects_count` optionally supply PDF `re`-operator rectangles for
/// rectangle-based table detection; pass NULL/0 to convert without it. The
/// array is borrowed only for the duration of the call.
/// `document_page_count` is the owning PDF's authoritative page count, so
/// trailing blank or unextracted pages count toward document-wide header,
/// footer, and folio coverage; pass 0 (it is a count, not a page number) to
/// fall back to the highest item page.
///
/// If `options` is NULL, defaults are used. Only Markdown settings are
/// observed; processing mode, detection settings, password, and page filters
/// are ignored. No path-line geometry or structure-tree context is available
/// on this path. The result must be freed with
/// `pdf_inspector_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_to_markdown_from_items(
    items: *const CTextItemsResult,
    rects: *const CPdfRect,
    rects_count: usize,
    document_page_count: u32,
    options: *const crate::PdfOptions,
    result_out: *mut *mut CTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let Some(items) = items.as_ref() else {
            return Err(PdfInspectorError::NullPointer);
        };
        let rects = pdf_rects_from_ffi(rects, rects_count)?;
        let options = markdown_options_or_default(options);
        let markdown = if document_page_count == 0 {
            crate::to_markdown_from_items_with_rects(items.items.clone(), options, &rects)
        } else {
            crate::to_markdown_from_items_with_rects_and_page_count(
                items.items.clone(),
                options,
                &rects,
                document_page_count,
            )
        };
        Ok(CTextResult { text: markdown })
    })
}

/// Extract plain text from a PDF file. `password` decrypts an encrypted PDF
/// (NULL = none; see `pdf_inspector_options_set_password`).
/// Populates `result_out` with an opaque `CTextResult` pointer; read the bytes
/// with `pdf_inspector_text_result_get_text`.
/// Must be freed with `pdf_inspector_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text(
    path: *const c_char,
    password: *const c_char,
    result_out: *mut *mut CTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let password = password_from_ffi(password)?;
        let text =
            crate::extractor::extract_text_with_password(path, password).map_err(map_error)?;
        Ok(CTextResult { text })
    })
}

/// Extract plain text from PDF bytes. A NULL buffer is accepted only when `size` is zero.
/// `password` decrypts an encrypted PDF (NULL = none; see `pdf_inspector_options_set_password`).
/// Populates `result_out` with an opaque `CTextResult` pointer; read the bytes
/// with `pdf_inspector_text_result_get_text`.
/// Must be freed with `pdf_inspector_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text_mem(
    buffer: *const u8,
    size: usize,
    password: *const c_char,
    result_out: *mut *mut CTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let password = password_from_ffi(password)?;
        let text = crate::extractor::extract_text_mem_with_password(buffer, password)
            .map_err(map_error)?;
        Ok(CTextResult { text })
    })
}

/// Extract positioned text items from a PDF file.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
/// `password` decrypts an encrypted PDF (NULL = none). Must be freed with
/// `pdf_inspector_text_items_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text_with_positions(
    path: *const c_char,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTextItemsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let page_filter = pages_set_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let items = crate::extractor::extract_text_with_positions_pages_with_password(
            path,
            page_filter.as_ref(),
            password,
        )
        .map_err(map_error)?;
        Ok(CTextItemsResult { items })
    })
}

/// Extract positioned text items from PDF bytes.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid. A NULL `buffer` is
/// accepted only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
/// The result must be freed with `pdf_inspector_text_items_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text_with_positions_mem(
    buffer: *const u8,
    size: usize,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTextItemsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let page_filter = pages_set_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let items = crate::extractor::extract_text_with_positions_mem_pages_with_password(
            buffer,
            page_filter.as_ref(),
            password,
        )
        .map_err(map_error)?;
        Ok(CTextItemsResult { items })
    })
}

/// Extract tagged-PDF structure-tree elements from a PDF file.
/// Returns an empty result for untagged PDFs. Entries are sorted by `(page, mcid)`; join those
/// fields against positioned text items to attach resolved standard or RoleMap roles to text.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
/// `password` decrypts an encrypted PDF (NULL = none). Must be freed with
/// `pdf_inspector_structure_elements_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_structure_elements(
    path: *const c_char,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CStructureElementsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let pages = pages_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let elements = crate::extract_structure_elements_with_password(path, pages, password)
            .map_err(map_error)?;
        Ok(CStructureElementsResult { elements })
    })
}

/// Extract tagged-PDF structure-tree elements from PDF bytes.
/// Returns an empty result for untagged PDFs. Entries are sorted by `(page, mcid)`; join those
/// fields against positioned text items to attach resolved standard or RoleMap roles to text.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid. A NULL `buffer` is
/// accepted only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
/// The result must be freed with `pdf_inspector_structure_elements_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_structure_elements_mem(
    buffer: *const u8,
    size: usize,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CStructureElementsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let pages = pages_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let elements = crate::extract_structure_elements_mem_with_password(buffer, pages, password)
            .map_err(map_error)?;
        Ok(CStructureElementsResult { elements })
    })
}

/// Extract pages markdown and metadata from a PDF file.
/// Populates `result_out` with `CPagesExtractionResult`.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
/// `password` decrypts an encrypted PDF (NULL = none).
/// Must be freed with `pdf_inspector_pages_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_pages_markdown(
    path: *const c_char,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CPagesExtractionResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let pages = pages_zero_indexed_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let inner = crate::extract_pages_markdown_with_password(path, pages.as_deref(), password)
            .map_err(map_error)?;
        Ok(CPagesExtractionResult { inner })
    })
}

/// Extract pages markdown and metadata from PDF bytes.
/// Populates `result_out` with `CPagesExtractionResult`.
/// `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
/// A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
/// A NULL `buffer` is accepted only when `size` is zero. `password` decrypts
/// an encrypted PDF (NULL = none).
/// Must be freed with `pdf_inspector_pages_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_pages_markdown_mem(
    buffer: *const u8,
    size: usize,
    pages: *const u32,
    pages_count: usize,
    password: *const c_char,
    result_out: *mut *mut CPagesExtractionResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let pages = pages_zero_indexed_from_ffi(pages, pages_count)?;
        let password = password_from_ffi(password)?;
        let inner =
            crate::extract_pages_markdown_mem_with_password(buffer, pages.as_deref(), password)
                .map_err(map_error)?;
        Ok(CPagesExtractionResult { inner })
    })
}

/// Extract text within bounding-box regions from a PDF file. Page numbers in
/// `page_regions` are 1-indexed; coordinates are PDF points with a top-left
/// origin. Results are parallel to the input pages and regions. `password`
/// decrypts an encrypted PDF (NULL = none). The result must be freed with
/// `pdf_inspector_region_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text_in_regions(
    path: *const c_char,
    page_regions: *const CPageRegions,
    page_regions_count: usize,
    password: *const c_char,
    result_out: *mut *mut CRegionTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let page_regions = page_regions_from_ffi(page_regions, page_regions_count)?;
        let password = password_from_ffi(password)?;
        let buffer = read_pdf_file(path)?;
        let pages =
            crate::extract_text_in_regions_mem_with_password(&buffer, &page_regions, password)
                .map_err(map_error)?;
        Ok(CRegionTextResult { pages })
    })
}

/// Extract text within bounding-box regions from PDF bytes. Page numbers in
/// `page_regions` are 1-indexed; coordinates are PDF points with a top-left
/// origin. A NULL `buffer` is accepted only when `size` is zero. `password`
/// decrypts an encrypted PDF (NULL = none). The result must be freed with
/// `pdf_inspector_region_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_text_in_regions_mem(
    buffer: *const u8,
    size: usize,
    page_regions: *const CPageRegions,
    page_regions_count: usize,
    password: *const c_char,
    result_out: *mut *mut CRegionTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let page_regions = page_regions_from_ffi(page_regions, page_regions_count)?;
        let password = password_from_ffi(password)?;
        let pages =
            crate::extract_text_in_regions_mem_with_password(buffer, &page_regions, password)
                .map_err(map_error)?;
        Ok(CRegionTextResult { pages })
    })
}

/// Extract markdown tables within bounding-box regions from a PDF file. Page
/// numbers in `page_regions` are 1-indexed; coordinates are PDF points with a
/// top-left origin. A region with no reliable table has empty text and
/// `needs_ocr` set. `password` decrypts an encrypted PDF (NULL = none). The
/// result must be freed with `pdf_inspector_region_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_in_regions(
    path: *const c_char,
    page_regions: *const CPageRegions,
    page_regions_count: usize,
    password: *const c_char,
    result_out: *mut *mut CRegionTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let page_regions = page_regions_from_ffi(page_regions, page_regions_count)?;
        let password = password_from_ffi(password)?;
        let buffer = read_pdf_file(path)?;
        let pages =
            crate::extract_tables_in_regions_mem_with_password(&buffer, &page_regions, password)
                .map_err(map_error)?;
        Ok(CRegionTextResult { pages })
    })
}

/// Extract markdown tables within bounding-box regions from PDF bytes. Page
/// numbers in `page_regions` are 1-indexed; coordinates are PDF points with a
/// top-left origin. A NULL `buffer` is accepted only when `size` is zero. A
/// region with no reliable table has empty text and `needs_ocr` set. `password`
/// decrypts an encrypted PDF (NULL = none). The result must be freed with
/// `pdf_inspector_region_text_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_in_regions_mem(
    buffer: *const u8,
    size: usize,
    page_regions: *const CPageRegions,
    page_regions_count: usize,
    password: *const c_char,
    result_out: *mut *mut CRegionTextResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let page_regions = page_regions_from_ffi(page_regions, page_regions_count)?;
        let password = password_from_ffi(password)?;
        let pages =
            crate::extract_tables_in_regions_mem_with_password(buffer, &page_regions, password)
                .map_err(map_error)?;
        Ok(CRegionTextResult { pages })
    })
}

/// Detect a vector ruled-line or rectangle grid inside one region of a PDF
/// file. `page` is 1-indexed. `region` uses PDF points with a top-left origin;
/// its corners may be supplied in either order. `render_dpi` must be finite and
/// positive, and the scaled crop dimensions must remain finite. It controls
/// the crop-pixel coordinates returned for cells.
/// `password` decrypts an encrypted PDF (NULL = none). Success always returns a
/// handle, including when no grid is detected. Free it with
/// `pdf_inspector_vector_grid_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_detect_vector_grid_in_region(
    path: *const c_char,
    page: u32,
    region: *const CRegion,
    render_dpi: f32,
    password: *const c_char,
    result_out: *mut *mut CVectorGridResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let (page, region) = vector_grid_request_from_ffi(page, region, render_dpi)?;
        let password = password_from_ffi(password)?;
        let buffer = read_pdf_file(path)?;
        let detection = crate::detect_vector_grid_in_region_mem_with_password(
            &buffer, page, region, render_dpi, password,
        )
        .map_err(map_error)?;
        Ok(CVectorGridResult { detection })
    })
}

/// Detect a vector ruled-line or rectangle grid inside one region of PDF
/// bytes. `page` is 1-indexed. `region` uses PDF points with a top-left origin;
/// its corners may be supplied in either order. `render_dpi` must be finite and
/// positive, and the scaled crop dimensions must remain finite. It controls
/// the crop-pixel coordinates returned for cells. A NULL `buffer` is accepted
/// only when `size` is zero. `password` decrypts an
/// encrypted PDF (NULL = none). Success always returns a handle, including when
/// no grid is detected. Free it with `pdf_inspector_vector_grid_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_detect_vector_grid_in_region_mem(
    buffer: *const u8,
    size: usize,
    page: u32,
    region: *const CRegion,
    render_dpi: f32,
    password: *const c_char,
    result_out: *mut *mut CVectorGridResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let (page, region) = vector_grid_request_from_ffi(page, region, render_dpi)?;
        let password = password_from_ffi(password)?;
        let detection = crate::detect_vector_grid_in_region_mem_with_password(
            buffer, page, region, render_dpi, password,
        )
        .map_err(map_error)?;
        Ok(CVectorGridResult { detection })
    })
}

/// Extract production-ready markdown tables using externally supplied table
/// structure recognition output. Pages are 1-indexed; crop coordinates are
/// PDF points with a top-left origin and ordered corners. Cell coordinates are
/// crop-image pixels and may contain 4-value rectangles or 8-value polygons.
/// Token and cell counts must match the parsed structure; row spans must fit
/// the declared rows and column spans are limited to 25. `password` decrypts an
/// encrypted PDF (NULL = none). Input arrays are borrowed only for this call.
/// Free the result with `pdf_inspector_tsr_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_with_structure_auto(
    path: *const c_char,
    inputs: *const CTsrTableInput,
    inputs_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTsrTableExtractionResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let inputs = tsr_inputs_from_ffi(inputs, inputs_count)?;
        let password = password_from_ffi(password)?;
        let buffer = read_pdf_file(path)?;
        let results =
            crate::extract_tables_with_structure_auto_mem_with_password(&buffer, &inputs, password)
                .map_err(map_error)?;
        Ok(CTsrTableExtractionResult { results })
    })
}

/// Extract production-ready markdown tables using externally supplied table
/// structure recognition output and PDF bytes. Pages are 1-indexed; crop
/// coordinates are PDF points with a top-left origin and ordered corners. Cell
/// coordinates are crop-image pixels and may contain 4-value rectangles or
/// 8-value polygons. Token and cell counts must match the parsed structure;
/// row spans must fit the declared rows and column spans are limited to 25. A
/// NULL `buffer` is accepted only when `size` is zero. `password` decrypts an
/// encrypted PDF (NULL = none). Input arrays are borrowed only for this call.
/// Free the result with `pdf_inspector_tsr_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_with_structure_auto_mem(
    buffer: *const u8,
    size: usize,
    inputs: *const CTsrTableInput,
    inputs_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTsrTableExtractionResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let inputs = tsr_inputs_from_ffi(inputs, inputs_count)?;
        let password = password_from_ffi(password)?;
        let results =
            crate::extract_tables_with_structure_auto_mem_with_password(buffer, &inputs, password)
                .map_err(map_error)?;
        Ok(CTsrTableExtractionResult { results })
    })
}

/// Resolve raw structured cells from externally supplied table-structure
/// recognition output. Pages are 1-indexed. Input geometry, token grammar,
/// span limits, and borrowing rules are the same as for
/// `pdf_inspector_extract_tables_with_structure_auto`. This path does not run
/// auto quality repair or heuristic fallback. `password` decrypts an encrypted
/// PDF (NULL = none). Free the result with `pdf_inspector_tsr_cells_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_with_structure_cells(
    path: *const c_char,
    inputs: *const CTsrTableInput,
    inputs_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTsrStructuredCellsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let path = str_from_ffi(path)?;
        let inputs = tsr_inputs_from_ffi(inputs, inputs_count)?;
        let password = password_from_ffi(password)?;
        let buffer = read_pdf_file(path)?;
        let tables = crate::extract_tables_with_structure_cells_mem_with_password(
            &buffer, &inputs, password,
        )
        .map_err(map_error)?;
        Ok(CTsrStructuredCellsResult { tables })
    })
}

/// Resolve raw structured cells from externally supplied table-structure
/// recognition output and PDF bytes. Pages are 1-indexed. Input geometry,
/// token grammar, span limits, and borrowing rules are the same as for
/// `pdf_inspector_extract_tables_with_structure_auto_mem`. This path does not
/// run auto quality repair or heuristic fallback. A NULL `buffer` is accepted
/// only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
/// Free the result with `pdf_inspector_tsr_cells_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_extract_tables_with_structure_cells_mem(
    buffer: *const u8,
    size: usize,
    inputs: *const CTsrTableInput,
    inputs_count: usize,
    password: *const c_char,
    result_out: *mut *mut CTsrStructuredCellsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || {
        let buffer = bytes_from_ffi(buffer, size)?;
        let inputs = tsr_inputs_from_ffi(inputs, inputs_count)?;
        let password = password_from_ffi(password)?;
        let tables =
            crate::extract_tables_with_structure_cells_mem_with_password(buffer, &inputs, password)
                .map_err(map_error)?;
        Ok(CTsrStructuredCellsResult { tables })
    })
}

// =========================================================================
// CTextResult Getters
// =========================================================================

/// Free a `CTextResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_result_free(result: *mut CTextResult) {
    free_boxed(result);
}

/// Get the extracted UTF-8 text bytes. Extracted text may legitimately contain
/// NUL bytes. Returns `false` and zeroes `out` for a NULL result or output.
/// The view remains valid until `result` is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_result_get_text(
    result: *const CTextResult,
    out: *mut CByteView,
) -> bool {
    byte_view_out(result.as_ref().map(|result| result.text.as_str()), out)
}

// =========================================================================
// CPdfProcessResult Getters
// =========================================================================

/// Free a `CPdfProcessResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_free(result: *mut CPdfProcessResult) {
    free_boxed(result);
}

/// Get the detected PDF type.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_type(
    result: *const CPdfProcessResult,
) -> CPdfType {
    with_result(result, CPdfType::Unknown, |res| c_pdf_type(res.pdf_type))
}

/// Get the total page count.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_page_count(
    result: *const CPdfProcessResult,
) -> u32 {
    with_result(result, 0, |res| res.page_count)
}

/// Get the processing time in milliseconds.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_processing_time_ms(
    result: *const CPdfProcessResult,
) -> u64 {
    with_result(result, 0, |res| res.processing_time_ms)
}

/// Get the confidence score (0.0 - 1.0).
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_confidence(
    result: *const CPdfProcessResult,
) -> f32 {
    with_result(result, 0.0, |res| res.confidence)
}

/// Returns true if encoding issues were detected.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_has_encoding_issues(
    result: *const CPdfProcessResult,
) -> bool {
    with_result(result, false, |res| res.has_encoding_issues)
}

/// Returns true if complex layout (tables or columns) was detected.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_is_complex_layout(
    result: *const CPdfProcessResult,
) -> bool {
    with_result(result, false, |res| res.layout.is_complex)
}

// =========================================================================
// CPdfClassification Getters
// =========================================================================

/// Free a `CPdfClassification` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classification_free(
    classification: *mut CPdfClassification,
) {
    free_boxed(classification);
}

/// Get the detected PDF type from classification.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classification_get_type(
    classification: *const CPdfClassification,
) -> CPdfType {
    with_classification(classification, CPdfType::Unknown, |cl| {
        c_pdf_type(cl.pdf_type)
    })
}

/// Get total page count from classification.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classification_get_page_count(
    classification: *const CPdfClassification,
) -> u32 {
    with_classification(classification, 0, |cl| cl.page_count)
}

/// Get confidence from classification.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classification_get_confidence(
    classification: *const CPdfClassification,
) -> f32 {
    with_classification(classification, 0.0, |cl| cl.confidence)
}

// =========================================================================
// CPdfTypeResult Getters
// =========================================================================

/// Free a `CPdfTypeResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_free(result: *mut CPdfTypeResult) {
    free_boxed(result);
}

/// Get the detected PDF type.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_type(
    result: *const CPdfTypeResult,
) -> CPdfType {
    with_pdf_type_result(result, CPdfType::Unknown, |result| {
        c_pdf_type(result.inner.pdf_type)
    })
}

/// Get the total number of pages in the document.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_page_count(
    result: *const CPdfTypeResult,
) -> u32 {
    with_pdf_type_result(result, 0, |result| result.inner.page_count)
}

/// Get the number of pages sampled during detection.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_pages_sampled(
    result: *const CPdfTypeResult,
) -> u32 {
    with_pdf_type_result(result, 0, |result| result.inner.pages_sampled)
}

/// Get the number of sampled pages classified as having text.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_pages_with_text(
    result: *const CPdfTypeResult,
) -> u32 {
    with_pdf_type_result(result, 0, |result| result.inner.pages_with_text)
}

/// Get the confidence score (0.0 - 1.0).
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_confidence(
    result: *const CPdfTypeResult,
) -> f32 {
    with_pdf_type_result(result, 0.0, |result| result.inner.confidence)
}

/// Get the optional document-title UTF-8 bytes. Returns `false` and zeroes
/// `out` when the title is absent or either pointer is NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_title(
    result: *const CPdfTypeResult,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.inner.title.as_deref()),
        out,
    )
}

/// Return whether OCR is recommended for better extraction.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_is_ocr_recommended(
    result: *const CPdfTypeResult,
) -> bool {
    with_pdf_type_result(result, false, |result| result.inner.ocr_recommended)
}

/// Get the borrowed array of 1-indexed page numbers needing OCR.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_pages_needing_ocr(
    result: *const CPdfTypeResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result
            .as_ref()
            .map(|result| &result.inner.pages_needing_ocr[..]),
        out,
    )
}

/// Get the number of entries in the per-page OCR reasons list.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_ocr_reasons_count(
    result: *const CPdfTypeResult,
) -> usize {
    with_pdf_type_result(result, 0, |result| result.ocr_reasons_by_page.len())
}

/// Get the 1-indexed page number for an OCR-reasons entry.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_ocr_reasons_page(
    result: *const CPdfTypeResult,
    index: usize,
) -> u32 {
    with_pdf_type_result(result, 0, |result| {
        ocr_reasons_page(&result.ocr_reasons_by_page, index)
    })
}

/// Get the number of reason strings for an OCR-reasons entry.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_ocr_reasons_reasons_count(
    result: *const CPdfTypeResult,
    index: usize,
) -> usize {
    with_pdf_type_result(result, 0, |result| {
        ocr_reasons_len(&result.ocr_reasons_by_page, index)
    })
}

/// Get one OCR reason's UTF-8 bytes. Returns `false` and zeroes `out` when the
/// requested reason is absent or an input pointer is NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pdf_type_result_get_ocr_reasons_reason(
    result: *const CPdfTypeResult,
    index: usize,
    reason_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result.as_ref().and_then(|result| {
            ocr_reason_at(&result.ocr_reasons_by_page, index, reason_index).map(String::as_str)
        }),
        out,
    )
}

// =========================================================================
// CPagesExtractionResult Getters
// =========================================================================

/// Free a `CPagesExtractionResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_free(result: *mut CPagesExtractionResult) {
    free_boxed(result);
}

/// Get number of extracted pages.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_page_count(
    result: *const CPagesExtractionResult,
) -> usize {
    with_pages_result(result, 0, |res| res.pages.len())
}

/// Get the 1-indexed page number of the page at `index`, matching the base used
/// by every other page number in this ABI. Returns 0 for an out-of-range index.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_page_number(
    result: *const CPagesExtractionResult,
    index: usize,
) -> u32 {
    // `PageMarkdown::page` is 0-indexed while the sibling page-number arrays on
    // the same struct are 1-indexed; normalise so C sees one base throughout.
    with_page(result, index, 0, |page| page.page.saturating_add(1))
}

/// Get whether page at index needs OCR.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_page_needs_ocr(
    result: *const CPagesExtractionResult,
    index: usize,
) -> bool {
    with_page(result, index, false, |page| page.needs_ocr)
}

/// Get whether any page has tables or columns.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_is_complex(
    result: *const CPagesExtractionResult,
) -> bool {
    with_pages_result(result, false, |res| res.is_complex)
}

// =========================================================================
// Advanced Zero-Copy and Detailed Getters for downstream language bindings
// =========================================================================

/// Get the Markdown UTF-8 bytes. Returns `false` and zeroes `out` when
/// Markdown is absent or either pointer is NULL. The view remains valid until
/// `result` is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_markdown(
    result: *const CPdfProcessResult,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.inner.markdown.as_deref()),
        out,
    )
}

/// Get the title UTF-8 bytes. Returns `false` and zeroes `out` when the title
/// is absent or either pointer is NULL. The view remains valid until `result`
/// is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_title(
    result: *const CPdfProcessResult,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.inner.title.as_deref()),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers needing OCR.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_pages_needing_ocr(
    result: *const CPdfProcessResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result.as_ref().map(|res| &res.inner.pages_needing_ocr[..]),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers with tables.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_pages_with_tables(
    result: *const CPdfProcessResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result
            .as_ref()
            .map(|res| &res.inner.layout.pages_with_tables[..]),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers with columns.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_pages_with_columns(
    result: *const CPdfProcessResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result
            .as_ref()
            .map(|res| &res.inner.layout.pages_with_columns[..]),
        out,
    )
}

/// Get the number of entries in the detailed OCR reasons list for `CPdfProcessResult`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_ocr_reasons_count(
    result: *const CPdfProcessResult,
) -> usize {
    with_result(result, 0, |res| res.ocr_reasons_by_page.len())
}

/// Get the 1-indexed page number for the OCR reason entry at index.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_ocr_reasons_page(
    result: *const CPdfProcessResult,
    index: usize,
) -> u32 {
    with_result(result, 0, |res| {
        ocr_reasons_page(&res.ocr_reasons_by_page, index)
    })
}

/// Get the number of OCR reason strings for the entry at index.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_ocr_reasons_reasons_count(
    result: *const CPdfProcessResult,
    index: usize,
) -> usize {
    with_result(result, 0, |res| {
        ocr_reasons_len(&res.ocr_reasons_by_page, index)
    })
}

/// Get an OCR reason's UTF-8 bytes. Returns `false` and zeroes `out` when the
/// requested reason is absent or an input pointer is NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_get_ocr_reasons_reason(
    result: *const CPdfProcessResult,
    index: usize,
    reason_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result.as_ref().and_then(|result| {
            ocr_reason_at(&result.inner.ocr_reasons_by_page, index, reason_index)
                .map(String::as_str)
        }),
        out,
    )
}

/// Get the page Markdown UTF-8 bytes at `index`. Returns `false` and zeroes
/// `out` for an invalid index or input pointer.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_page_markdown(
    result: *const CPagesExtractionResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.inner.pages.get(index))
            .map(|page| page.markdown.as_str()),
        out,
    )
}

/// Get the page OCR reason UTF-8 bytes at `index`. Returns `false` and zeroes
/// `out` when the reason is absent or an input pointer is invalid.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_page_ocr_reason(
    result: *const CPagesExtractionResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.inner.pages.get(index))
            .and_then(|page| page.ocr_reason.as_deref()),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers needing OCR.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_pages_needing_ocr(
    result: *const CPagesExtractionResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result.as_ref().map(|res| &res.inner.pages_needing_ocr[..]),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers with tables.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_pages_with_tables(
    result: *const CPagesExtractionResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result.as_ref().map(|res| &res.inner.pages_with_tables[..]),
        out,
    )
}

/// Get the borrowed array of 1-indexed page numbers with columns.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_pages_with_columns(
    result: *const CPagesExtractionResult,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        result.as_ref().map(|res| &res.inner.pages_with_columns[..]),
        out,
    )
}

/// Get the number of entries in the detailed OCR reasons list for `CPagesExtractionResult`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_ocr_reasons_count(
    result: *const CPagesExtractionResult,
) -> usize {
    with_pages_result(result, 0, |res| res.ocr_reasons_by_page.len())
}

/// Get the 1-indexed page number for the OCR reason entry at index in `CPagesExtractionResult`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_ocr_reasons_page(
    result: *const CPagesExtractionResult,
    index: usize,
) -> u32 {
    with_pages_result(result, 0, |res| {
        ocr_reasons_page(&res.ocr_reasons_by_page, index)
    })
}

/// Get the number of OCR reason strings for the entry at index in `CPagesExtractionResult`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_ocr_reasons_reasons_count(
    result: *const CPagesExtractionResult,
    index: usize,
) -> usize {
    with_pages_result(result, 0, |res| {
        ocr_reasons_len(&res.ocr_reasons_by_page, index)
    })
}

/// Get an OCR reason's UTF-8 bytes. Returns `false` and zeroes `out` when the
/// requested reason is absent or an input pointer is NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_pages_result_get_ocr_reasons_reason(
    result: *const CPagesExtractionResult,
    index: usize,
    reason_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result.as_ref().and_then(|result| {
            ocr_reason_at(&result.inner.ocr_reasons_by_page, index, reason_index)
                .map(String::as_str)
        }),
        out,
    )
}

/// Get the borrowed array of page numbers needing OCR, 1-indexed like every
/// other page number in this ABI.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_classification_get_pages_needing_ocr(
    classification: *const CPdfClassification,
    out: *mut CU32View,
) -> bool {
    u32_view_out(
        classification
            .as_ref()
            .map(|cl| &cl.inner.pages_needing_ocr[..]),
        out,
    )
}

// =========================================================================
// CTextItemsResult builder and getters
// =========================================================================

/// Create an empty caller-owned `CTextItemsResult`. Populate it with
/// `pdf_inspector_text_items_result_add`; it is then accepted everywhere an
/// extracted `CTextItemsResult` is (getters,
/// `pdf_inspector_to_markdown_from_items`). This is the entry point for
/// feeding externally produced positioned text — e.g. OCR output for regions
/// reported as `needs_ocr` — back through the Markdown converter.
/// Must be freed with `pdf_inspector_text_items_result_free`.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_new(
    result_out: *mut *mut CTextItemsResult,
) -> PdfInspectorError {
    emit_handle(result_out, || Ok(CTextItemsResult { items: Vec::new() }))
}

/// Append `descriptors_count` caller-supplied items to a `CTextItemsResult`,
/// copying every string, so the descriptor array is borrowed only for the
/// duration of the call. `descriptors` may be NULL only when
/// `descriptors_count` is zero. The call is atomic: on any error nothing is
/// appended. Items may be added in any order across any pages; the converter
/// sorts by position. Must not race with another use of the same handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_add(
    items: *mut CTextItemsResult,
    descriptors: *const CTextItemDescriptor,
    descriptors_count: usize,
) -> PdfInspectorError {
    catch_panic_err(|| {
        let Some(items) = items.as_mut() else {
            return Err(PdfInspectorError::NullPointer);
        };
        let converted = text_items_from_ffi(descriptors, descriptors_count)?;
        items.items.extend(converted);
        Ok(())
    })
}

/// Free a `CTextItemsResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_free(result: *mut CTextItemsResult) {
    free_boxed(result);
}

/// Get the number of positioned text items.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_count(
    result: *const CTextItemsResult,
) -> usize {
    result.as_ref().map_or(0, |result| result.items.len())
}

/// Get an item's 1-indexed page number.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_page(
    result: *const CTextItemsResult,
    index: usize,
) -> u32 {
    with_text_item(result, index, 0, |item| item.page)
}

/// Get an item's UTF-8 text bytes. Returns `false` and zeroes `out` for an
/// invalid item index or input pointer. The view remains valid until `result`
/// is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_text(
    result: *const CTextItemsResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.items.get(index))
            .map(|item| item.text.as_str()),
        out,
    )
}

/// Get an item's x coordinate in PDF points.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_x(
    result: *const CTextItemsResult,
    index: usize,
) -> f32 {
    with_text_item(result, index, 0.0, |item| item.x)
}

/// Get an item's y coordinate in PDF points.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_y(
    result: *const CTextItemsResult,
    index: usize,
) -> f32 {
    with_text_item(result, index, 0.0, |item| item.y)
}

/// Get an item's width in PDF points.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_width(
    result: *const CTextItemsResult,
    index: usize,
) -> f32 {
    with_text_item(result, index, 0.0, |item| item.width)
}

/// Get an item's height in PDF points.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_height(
    result: *const CTextItemsResult,
    index: usize,
) -> f32 {
    with_text_item(result, index, 0.0, |item| item.height)
}

/// Get an item's font-name UTF-8 bytes. Returns `false` and zeroes `out` for an
/// invalid item index or input pointer. The view remains valid until `result`
/// is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_font(
    result: *const CTextItemsResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.items.get(index))
            .map(|item| item.font.as_str()),
        out,
    )
}

/// Get an item's font size.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_font_size(
    result: *const CTextItemsResult,
    index: usize,
) -> f32 {
    with_text_item(result, index, 0.0, |item| item.font_size)
}

/// Get an item's type.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_type(
    result: *const CTextItemsResult,
    index: usize,
) -> CTextItemType {
    with_text_item(result, index, CTextItemType::Unknown, |item| {
        text_item_type(&item.item_type)
    })
}

/// Return whether an item is bold, italic, underlined, or struck out, respectively.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_is_bold(
    result: *const CTextItemsResult,
    index: usize,
) -> bool {
    with_text_item(result, index, false, |item| item.is_bold)
}

#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_is_italic(
    result: *const CTextItemsResult,
    index: usize,
) -> bool {
    with_text_item(result, index, false, |item| item.is_italic)
}

#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_is_underline(
    result: *const CTextItemsResult,
    index: usize,
) -> bool {
    with_text_item(result, index, false, |item| item.is_underline)
}

#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_is_strikeout(
    result: *const CTextItemsResult,
    index: usize,
) -> bool {
    with_text_item(result, index, false, |item| item.is_strikeout)
}

/// Get a link item's URL UTF-8 bytes. Returns `false` and zeroes `out` for a
/// non-link item, invalid index, or input pointer. The view remains valid until
/// `result` is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_link_url(
    result: *const CTextItemsResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.items.get(index))
            .and_then(|item| text_item_link_url(&item.item_type))
            .map(String::as_str),
        out,
    )
}

/// Return whether an item has a marked-content ID (MCID).
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_has_mcid(
    result: *const CTextItemsResult,
    index: usize,
) -> bool {
    with_text_item(result, index, false, |item| item.mcid.is_some())
}

/// Get an item's MCID. Check `pdf_inspector_text_items_result_has_mcid` first.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_text_items_result_get_mcid(
    result: *const CTextItemsResult,
    index: usize,
) -> i64 {
    with_text_item(result, index, 0, |item| item.mcid.unwrap_or(0))
}

// =========================================================================
// CStructureElementsResult Getters
// =========================================================================

/// Free a `CStructureElementsResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_structure_elements_result_free(
    result: *mut CStructureElementsResult,
) {
    free_boxed(result);
}

/// Get the number of tagged-PDF structure elements.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_structure_elements_result_get_count(
    result: *const CStructureElementsResult,
) -> usize {
    result.as_ref().map_or(0, |result| result.elements.len())
}

/// Get a structure element's 1-indexed page number.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_structure_elements_result_get_page(
    result: *const CStructureElementsResult,
    index: usize,
) -> u32 {
    with_structure_element(result, index, 0, |element| element.page)
}

/// Get a structure element's marked-content ID.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_structure_elements_result_get_mcid(
    result: *const CStructureElementsResult,
    index: usize,
) -> i64 {
    with_structure_element(result, index, 0, |element| element.mcid)
}

/// Get a structure element's role UTF-8 bytes. Returns `false` and zeroes `out`
/// for an invalid element index or input pointer. The view remains valid until
/// `result` is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_structure_elements_result_get_role(
    result: *const CStructureElementsResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.elements.get(index))
            .map(|element| element.role.as_str()),
        out,
    )
}

// =========================================================================
// CRegionTextResult Getters
// =========================================================================

/// Free a `CRegionTextResult` instance.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_free(result: *mut CRegionTextResult) {
    free_boxed(result);
}

/// Get the number of page entries in a region-text result.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_get_page_count(
    result: *const CRegionTextResult,
) -> usize {
    result.as_ref().map_or(0, |result| result.pages.len())
}

/// Get a page entry's 1-indexed page number.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_get_page(
    result: *const CRegionTextResult,
    page_index: usize,
) -> u32 {
    with_region_page(result, page_index, 0, |page| page.page.saturating_add(1))
}

/// Get the number of region entries for one page entry.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_get_region_count(
    result: *const CRegionTextResult,
    page_index: usize,
) -> usize {
    with_region_page(result, page_index, 0, |page| page.regions.len())
}

/// Get a region's extracted UTF-8 text bytes. Returns `false` and zeroes `out`
/// for an invalid index or input pointer. The view remains valid until `result`
/// is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_get_text(
    result: *const CRegionTextResult,
    page_index: usize,
    region_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.pages.get(page_index))
            .and_then(|page| page.regions.get(region_index))
            .map(|region| region.text.as_str()),
        out,
    )
}

/// Return whether a region's extracted text is unreliable and should be
/// replaced with OCR.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_needs_ocr(
    result: *const CRegionTextResult,
    page_index: usize,
    region_index: usize,
) -> bool {
    with_region(result, page_index, region_index, false, |region| {
        region.needs_ocr
    })
}

/// Get a region's optional machine-readable OCR-reason UTF-8 bytes. Returns
/// `false` and zeroes `out` when no reason is available or an input index or
/// pointer is invalid. The view remains valid until `result` is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_region_text_result_get_ocr_reason(
    result: *const CRegionTextResult,
    page_index: usize,
    region_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.pages.get(page_index))
            .and_then(|page| page.regions.get(region_index))
            .and_then(|region| region.ocr_reason.as_deref()),
        out,
    )
}

// =========================================================================
// CVectorGridResult Getters
// =========================================================================

/// Free a `CVectorGridResult` instance. NULL is accepted.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_free(result: *mut CVectorGridResult) {
    free_boxed(result);
}

/// Return whether a grid was detected. A valid no-grid result returns false,
/// as does a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_is_detected(
    result: *const CVectorGridResult,
) -> bool {
    result
        .as_ref()
        .is_some_and(|result| result.detection.is_some())
}

/// Get the number of HTML-like structure tokens in a detected grid. Returns
/// zero for a no-grid result or NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_get_structure_token_count(
    result: *const CVectorGridResult,
) -> usize {
    result
        .as_ref()
        .and_then(|result| result.detection.as_ref())
        .map_or(0, |detection| detection.structure_tokens.len())
}

/// Get one borrowed UTF-8 structure token. Returns false and zeroes `out` for
/// a no-grid result, invalid index, NULL handle, or NULL output. The view stays
/// valid until the result handle is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_get_structure_token(
    result: *const CVectorGridResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.detection.as_ref())
            .and_then(|detection| detection.structure_tokens.get(index))
            .map(String::as_str),
        out,
    )
}

/// Get the number of crop-pixel cell boxes in a detected grid. Returns zero
/// for a no-grid result or NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_get_cell_count(
    result: *const CVectorGridResult,
) -> usize {
    result
        .as_ref()
        .and_then(|result| result.detection.as_ref())
        .map_or(0, |detection| detection.cell_bboxes.len())
}

/// Copy one detected cell box, in crop-image pixels with a top-left origin,
/// into `out`. Returns false and zeroes `out` for a no-grid result, malformed
/// or invalid index, NULL handle, or NULL output.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_vector_grid_result_get_cell_box(
    result: *const CVectorGridResult,
    index: usize,
    out: *mut CVectorGridCellBox,
) -> bool {
    let Some(out) = out.as_mut() else {
        return false;
    };
    *out = CVectorGridCellBox::default();
    let Some(bbox) = result
        .as_ref()
        .and_then(|result| result.detection.as_ref())
        .and_then(|detection| detection.cell_bboxes.get(index))
    else {
        return false;
    };
    let [x1, y1, x2, y2] = bbox.as_slice() else {
        return false;
    };
    *out = CVectorGridCellBox {
        x1: *x1,
        y1: *y1,
        x2: *x2,
        y2: *y2,
    };
    true
}

// =========================================================================
// CTsrTableExtractionResult Getters
// =========================================================================

/// Free a `CTsrTableExtractionResult` instance. NULL is accepted.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_result_free(result: *mut CTsrTableExtractionResult) {
    free_boxed(result);
}

/// Get the number of table extraction results. Returns zero for a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_result_get_count(
    result: *const CTsrTableExtractionResult,
) -> usize {
    result.as_ref().map_or(0, |result| result.results.len())
}

/// Get one borrowed Markdown string. Returns false and zeroes `out` for an
/// invalid index, NULL handle, or NULL output. Empty Markdown is present.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_result_get_markdown(
    result: *const CTsrTableExtractionResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.results.get(index))
            .map(|result| result.markdown.as_str()),
        out,
    )
}

/// Get one optional borrowed fallback-reason label. Returns false and zeroes
/// `out` when no fallback occurred, or for an invalid index or NULL pointer.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_result_get_fallback_reason(
    result: *const CTsrTableExtractionResult,
    index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.results.get(index))
            .and_then(|result| result.fallback_reason.as_deref()),
        out,
    )
}

// =========================================================================
// CTsrStructuredCellsResult Getters
// =========================================================================

/// Free a `CTsrStructuredCellsResult` instance. NULL is accepted.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_cells_result_free(
    result: *mut CTsrStructuredCellsResult,
) {
    free_boxed(result);
}

/// Get the number of input-parallel cell lists. Returns zero for a NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_cells_result_get_table_count(
    result: *const CTsrStructuredCellsResult,
) -> usize {
    result.as_ref().map_or(0, |result| result.tables.len())
}

/// Get the number of cells for one input. Returns zero for an invalid index or
/// NULL handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_cells_result_get_cell_count(
    result: *const CTsrStructuredCellsResult,
    table_index: usize,
) -> usize {
    result
        .as_ref()
        .and_then(|result| result.tables.get(table_index))
        .map_or(0, Vec::len)
}

/// Copy fixed metadata for one cell into `out`. Returns false and zeroes `out`
/// for invalid indices or NULL pointers.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_cells_result_get_cell(
    result: *const CTsrStructuredCellsResult,
    table_index: usize,
    cell_index: usize,
    out: *mut CTsrStructuredCell,
) -> bool {
    let Some(out) = out.as_mut() else {
        return false;
    };
    *out = CTsrStructuredCell::default();
    let Some(cell) = result
        .as_ref()
        .and_then(|result| result.tables.get(table_index))
        .and_then(|table| table.get(cell_index))
    else {
        return false;
    };
    let [x1, y1, x2, y2] = cell.page_pt_bbox;
    *out = CTsrStructuredCell {
        row: cell.row,
        col: cell.col,
        rowspan: cell.rowspan,
        colspan: cell.colspan,
        is_header: cell.is_header,
        page_pt_bbox: CRegion { x1, y1, x2, y2 },
    };
    true
}

/// Get one borrowed UTF-8 cell-text view. Returns false and zeroes `out` for
/// invalid indices or NULL pointers. Empty cell text is present. The view
/// remains valid until the result handle is freed.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_tsr_cells_result_get_cell_text(
    result: *const CTsrStructuredCellsResult,
    table_index: usize,
    cell_index: usize,
    out: *mut CByteView,
) -> bool {
    byte_view_out(
        result
            .as_ref()
            .and_then(|result| result.tables.get(table_index))
            .and_then(|table| table.get(cell_index))
            .map(|cell| cell.text.as_str()),
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    const FIXTURE: &str = "tests/fixtures/bare_name_struct.pdf";
    const TAGGED_FIXTURE: &str = "tests/fixtures/firecrawl_docs_tagged.pdf";

    fn get_byte_view(call: impl FnOnce(*mut CByteView) -> bool) -> Option<CByteView> {
        let mut view = CByteView::default();
        call(&mut view).then_some(view)
    }

    fn get_u32_view(call: impl FnOnce(*mut CU32View) -> bool) -> Option<CU32View> {
        let mut view = CU32View::default();
        call(&mut view).then_some(view)
    }

    type BoolSetter = unsafe extern "C" fn(*mut crate::PdfOptions, bool) -> PdfInspectorError;

    /// Every `bool` option setter, so a new one is visibly missing from the list.
    const BOOL_SETTERS: &[(&str, BoolSetter)] = &[
        ("detect_headers", pdf_inspector_options_set_detect_headers),
        ("detect_lists", pdf_inspector_options_set_detect_lists),
        ("detect_code", pdf_inspector_options_set_detect_code),
        (
            "remove_page_numbers",
            pdf_inspector_options_set_remove_page_numbers,
        ),
        ("format_urls", pdf_inspector_options_set_format_urls),
        ("fix_hyphenation", pdf_inspector_options_set_fix_hyphenation),
        ("detect_bold", pdf_inspector_options_set_detect_bold),
        ("detect_italic", pdf_inspector_options_set_detect_italic),
        (
            "detect_underline",
            pdf_inspector_options_set_detect_underline,
        ),
        ("include_images", pdf_inspector_options_set_include_images),
        ("include_links", pdf_inspector_options_set_include_links),
        (
            "include_page_numbers",
            pdf_inspector_options_set_include_page_numbers,
        ),
        (
            "strip_headers_footers",
            pdf_inspector_options_set_strip_headers_footers,
        ),
    ];

    #[test]
    fn test_c_api_basic() {
        unsafe {
            let options = pdf_inspector_options_new();
            assert!(!options.is_null());

            let err = pdf_inspector_options_set_mode(options, 2); // Full
            assert_eq!(err, PdfInspectorError::Success);

            let err_pwd = pdf_inspector_options_set_password(options, std::ptr::null());
            assert_eq!(err_pwd, PdfInspectorError::Success);

            let err_page = pdf_inspector_options_add_page(options, 1);
            assert_eq!(err_page, PdfInspectorError::Success);

            // Path to a valid PDF fixture
            let path = CString::new(FIXTURE).unwrap();
            let mut result_ptr = std::ptr::null_mut();

            let err_proc = pdf_inspector_process_pdf(path.as_ptr(), options, &mut result_ptr);
            assert_eq!(err_proc, PdfInspectorError::Success);
            assert!(!result_ptr.is_null());

            // Query results
            let pdf_type = pdf_inspector_result_get_type(result_ptr);
            assert_eq!(pdf_type, CPdfType::TextBased);

            let page_count = pdf_inspector_result_get_page_count(result_ptr);
            assert!(page_count > 0);

            let markdown =
                get_byte_view(|out| pdf_inspector_result_get_markdown(result_ptr, out)).unwrap();
            assert!(!markdown.ptr.is_null());
            assert!(markdown.len > 0);

            // Free result
            pdf_inspector_result_free(result_ptr);
            pdf_inspector_options_free(options);
        }
    }

    #[test]
    fn test_c_api_classify_and_text() {
        unsafe {
            // Read PDF bytes
            let bytes = std::fs::read(FIXTURE).unwrap();
            let mut classification_ptr = std::ptr::null_mut();

            let err = pdf_inspector_classify_pdf_mem(
                bytes.as_ptr(),
                bytes.len(),
                std::ptr::null(),
                &mut classification_ptr,
            );
            assert_eq!(err, PdfInspectorError::Success);
            assert!(!classification_ptr.is_null());

            let page_count = pdf_inspector_classification_get_page_count(classification_ptr);
            assert!(page_count > 0);

            pdf_inspector_classification_free(classification_ptr);

            // Extract text
            let mut text_result = std::ptr::null_mut();
            let err_txt = pdf_inspector_extract_text_mem(
                bytes.as_ptr(),
                bytes.len(),
                std::ptr::null(),
                &mut text_result,
            );
            assert_eq!(err_txt, PdfInspectorError::Success);
            assert!(!text_result.is_null());
            let text =
                get_byte_view(|out| pdf_inspector_text_result_get_text(text_result, out)).unwrap();
            assert!(text.len > 0);
            assert!(!text.ptr.is_null());

            // Free extracted text
            pdf_inspector_text_result_free(text_result);
        }
    }

    #[test]
    fn plain_text_markdown_ffi_validates_utf8_and_applies_markdown_options() {
        unsafe {
            let input = "• First item\nplain\0text\n";
            let mut result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown(
                    input.as_ptr(),
                    input.len(),
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            assert_eq!(
                std::slice::from_raw_parts(markdown.ptr, markdown.len),
                b"- First item\nplain\0text\n"
            );
            pdf_inspector_text_result_free(result);

            let options = pdf_inspector_options_new();
            assert!(!options.is_null());
            assert_eq!(
                pdf_inspector_options_set_detect_lists(options, false),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_options_set_mode(options, CProcessMode::DetectOnly as i32),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_options_add_page(options, 3),
                PdfInspectorError::Success
            );
            result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown(input.as_ptr(), input.len(), options, &mut result),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            assert_eq!(
                std::slice::from_raw_parts(markdown.ptr, markdown.len),
                input.as_bytes()
            );
            pdf_inspector_text_result_free(result);
            pdf_inspector_options_free(options);

            result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_to_markdown(std::ptr::null(), 0, std::ptr::null(), &mut result),
                PdfInspectorError::Success
            );
            assert!(!result.is_null());
            let empty =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            assert!(!empty.ptr.is_null());
            assert_eq!(empty.len, 0);
            pdf_inspector_text_result_free(result);

            let invalid_utf8 = [0xff];
            result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_to_markdown(
                    invalid_utf8.as_ptr(),
                    invalid_utf8.len(),
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::InvalidUtf8
            );
            assert!(result.is_null());
            assert_eq!(
                pdf_inspector_to_markdown(std::ptr::null(), 1, std::ptr::null(), &mut result),
                PdfInspectorError::NullPointer
            );
            assert!(result.is_null());
        }
    }

    #[test]
    fn positioned_items_markdown_ffi_preserves_source_and_applies_options() {
        unsafe {
            let make_item = |text: &str, y: f32, font_size: f32| crate::TextItem {
                text: text.to_string(),
                x: 100.0,
                y,
                width: 100.0,
                height: font_size,
                font: "Arial".to_string(),
                font_size,
                page: 1,
                is_bold: false,
                is_italic: false,
                is_underline: false,
                is_strikeout: false,
                item_type: crate::types::ItemType::Text,
                mcid: None,
            };
            let items = Box::into_raw(Box::new(CTextItemsResult {
                items: vec![
                    make_item("Title", 750.0, 24.0),
                    make_item("Body text one", 700.0, 12.0),
                    make_item("Body text two", 680.0, 12.0),
                    make_item("Body text three", 660.0, 12.0),
                ],
            }));

            let mut result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    items,
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            let markdown = std::slice::from_raw_parts(markdown.ptr, markdown.len);
            assert!(markdown.windows(b"# Title".len()).any(|w| w == b"# Title"));
            assert_eq!(pdf_inspector_text_items_result_get_count(items), 4);
            pdf_inspector_text_result_free(result);

            let options = pdf_inspector_options_new();
            assert_eq!(
                pdf_inspector_options_set_detect_headers(options, false),
                PdfInspectorError::Success
            );
            result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    items,
                    std::ptr::null(),
                    0,
                    0,
                    options,
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            let markdown = std::slice::from_raw_parts(markdown.ptr, markdown.len);
            assert!(!markdown.windows(b"# Title".len()).any(|w| w == b"# Title"));
            assert!(markdown.windows(b"Title".len()).any(|w| w == b"Title"));
            assert_eq!(pdf_inspector_text_items_result_get_count(items), 4);
            pdf_inspector_text_result_free(result);
            pdf_inspector_options_free(options);

            let empty_items = Box::into_raw(Box::new(CTextItemsResult { items: Vec::new() }));
            result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    empty_items,
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            assert!(!markdown.ptr.is_null());
            assert_eq!(markdown.len, 0);
            pdf_inspector_text_result_free(result);
            pdf_inspector_text_items_result_free(empty_items);

            result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::NullPointer
            );
            assert!(result.is_null());

            // Rect validation: non-finite coordinates are rejected and a NULL
            // rect array is accepted only with a zero count. (Rect page 0 is
            // covered by `page_zero_is_rejected_at_every_page_list_entry_point`.)
            let nan_rect = CPdfRect {
                page: 1,
                x: f32::NAN,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            };
            for rects in [&nan_rect as *const CPdfRect, std::ptr::null()] {
                result = std::ptr::NonNull::dangling().as_ptr();
                assert_eq!(
                    pdf_inspector_to_markdown_from_items(
                        items,
                        rects,
                        1,
                        0,
                        std::ptr::null(),
                        &mut result,
                    ),
                    PdfInspectorError::InvalidArgument
                );
                assert!(result.is_null());
            }

            // A valid rect array and an explicit page count both convert.
            let rect = CPdfRect {
                page: 1,
                x: 90.0,
                y: 640.0,
                width: 120.0,
                height: 20.0,
            };
            result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    items,
                    &rect,
                    1,
                    3,
                    std::ptr::null(),
                    &mut result
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            assert!(markdown.len > 0);
            pdf_inspector_text_result_free(result);

            pdf_inspector_text_items_result_free(items);
        }
    }

    #[test]
    fn caller_built_text_items_ffi_validates_and_round_trips() {
        unsafe {
            let view = |s: &str| CByteView {
                ptr: s.as_ptr(),
                len: s.len(),
            };
            let make_descriptor =
                |text: &'static str, y: f32, font_size: f32| CTextItemDescriptor {
                    page: 1,
                    text: view(text),
                    x: 100.0,
                    y,
                    width: 100.0,
                    height: font_size,
                    font: view("OcrFont"),
                    font_size,
                    item_type: CTextItemType::Text as i32,
                    link_url: CByteView::default(),
                    flags: 0,
                    mcid: 0,
                };

            let mut items = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_text_items_result_new(&mut items),
                PdfInspectorError::Success
            );
            assert!(!items.is_null());
            assert_eq!(pdf_inspector_text_items_result_get_count(items), 0);

            // NULL descriptors are accepted only with a zero count.
            assert_eq!(
                pdf_inspector_text_items_result_add(items, std::ptr::null(), 0),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_text_items_result_add(items, std::ptr::null(), 1),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(
                pdf_inspector_text_items_result_add(std::ptr::null_mut(), std::ptr::null(), 0),
                PdfInspectorError::NullPointer
            );

            let mut bold_title = make_descriptor("Title", 750.0, 24.0);
            bold_title.flags =
                PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD | PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID;
            bold_title.mcid = 7;
            let mut link = make_descriptor("docs", 700.0, 12.0);
            link.item_type = CTextItemType::Link as i32;
            link.link_url = view("https://example.com");
            let batch = [
                bold_title,
                link,
                make_descriptor("Body text one", 680.0, 12.0),
                make_descriptor("Body text two", 660.0, 12.0),
            ];
            assert_eq!(
                pdf_inspector_text_items_result_add(items, batch.as_ptr(), batch.len()),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_text_items_result_get_count(items), 4);
            assert_eq!(pdf_inspector_text_items_result_get_page(items, 0), 1);
            assert!(pdf_inspector_text_items_result_is_bold(items, 0));
            assert!(!pdf_inspector_text_items_result_is_bold(items, 1));
            assert!(pdf_inspector_text_items_result_has_mcid(items, 0));
            assert_eq!(pdf_inspector_text_items_result_get_mcid(items, 0), 7);
            assert!(!pdf_inspector_text_items_result_has_mcid(items, 1));
            assert_eq!(
                pdf_inspector_text_items_result_get_type(items, 1),
                CTextItemType::Link
            );
            let url =
                get_byte_view(|out| pdf_inspector_text_items_result_get_link_url(items, 1, out))
                    .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(url.ptr, url.len),
                b"https://example.com"
            );
            let text = get_byte_view(|out| pdf_inspector_text_items_result_get_text(items, 0, out))
                .unwrap();
            assert_eq!(std::slice::from_raw_parts(text.ptr, text.len), b"Title");

            // Each rejected descriptor fails the whole batch atomically.
            // (Descriptor page 0 is covered by
            // `page_zero_is_rejected_at_every_page_list_entry_point`.)
            let invalid_utf8 = [0xffu8];
            let rejected = [
                (
                    {
                        let mut d = make_descriptor("unknown flag", 100.0, 12.0);
                        d.flags = TEXT_ITEM_FLAGS_ALL + 1;
                        d
                    },
                    PdfInspectorError::InvalidArgument,
                ),
                (
                    {
                        let mut d = make_descriptor("unknown item type", 100.0, 12.0);
                        d.item_type = CTextItemType::Unknown as i32;
                        d
                    },
                    PdfInspectorError::InvalidArgument,
                ),
                (
                    {
                        let mut d = make_descriptor("non-finite", 100.0, 12.0);
                        d.font_size = f32::NAN;
                        d
                    },
                    PdfInspectorError::InvalidArgument,
                ),
                (
                    {
                        let mut d = make_descriptor("", 100.0, 12.0);
                        d.text = CByteView {
                            ptr: invalid_utf8.as_ptr(),
                            len: invalid_utf8.len(),
                        };
                        d
                    },
                    PdfInspectorError::InvalidUtf8,
                ),
                (
                    {
                        let mut d = make_descriptor("", 100.0, 12.0);
                        d.text = CByteView {
                            ptr: std::ptr::null(),
                            len: 1,
                        };
                        d
                    },
                    PdfInspectorError::NullPointer,
                ),
            ];
            for (descriptor, expected) in rejected {
                assert_eq!(
                    pdf_inspector_text_items_result_add(
                        items,
                        [make_descriptor("valid", 100.0, 12.0), descriptor].as_ptr(),
                        2,
                    ),
                    expected
                );
                assert_eq!(pdf_inspector_text_items_result_get_count(items), 4);
            }

            // The built handle feeds the Markdown converter like an extracted one.
            let mut result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    items,
                    std::ptr::null(),
                    0,
                    0,
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            let markdown =
                get_byte_view(|out| pdf_inspector_text_result_get_text(result, out)).unwrap();
            let markdown = std::slice::from_raw_parts(markdown.ptr, markdown.len);
            assert!(markdown.windows(b"Title".len()).any(|w| w == b"Title"));
            assert!(markdown
                .windows(b"Body text one".len())
                .any(|w| w == b"Body text one"));
            pdf_inspector_text_result_free(result);
            pdf_inspector_text_items_result_free(items);
        }
    }

    #[test]
    fn full_detector_ffi_supports_path_memory_options_and_password() {
        unsafe {
            let options = pdf_inspector_options_new();
            assert!(!options.is_null());
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Full as i32,
                    0,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::Success
            );

            let path = CString::new(FIXTURE).unwrap();
            let mut path_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_pdf_type(path.as_ptr(), options, &mut path_result),
                PdfInspectorError::Success
            );
            assert!(!path_result.is_null());
            assert_eq!(
                pdf_inspector_pdf_type_result_get_type(path_result),
                CPdfType::TextBased
            );
            assert!(pdf_inspector_pdf_type_result_get_page_count(path_result) > 0);
            assert_eq!(
                pdf_inspector_pdf_type_result_get_pages_sampled(path_result),
                pdf_inspector_pdf_type_result_get_page_count(path_result)
            );
            assert!(pdf_inspector_pdf_type_result_get_confidence(path_result) > 0.0);

            let encrypted = std::fs::read("tests/fixtures/encrypted-secret123.pdf").unwrap();
            let wrong_password = CString::new("wrong").unwrap();
            assert_eq!(
                pdf_inspector_options_set_password(options, wrong_password.as_ptr()),
                PdfInspectorError::Success
            );
            let mut mem_result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_detect_pdf_type_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    options,
                    &mut mem_result,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(mem_result.is_null());

            let password = CString::new("secret123").unwrap();
            assert_eq!(
                pdf_inspector_options_set_password(options, password.as_ptr()),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_detect_pdf_type_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    options,
                    &mut mem_result,
                ),
                PdfInspectorError::Success
            );
            assert!(!mem_result.is_null());
            assert!(pdf_inspector_pdf_type_result_get_page_count(mem_result) > 0);

            pdf_inspector_pdf_type_result_free(mem_result);
            pdf_inspector_pdf_type_result_free(path_result);
            pdf_inspector_options_free(options);
        }
    }

    #[test]
    fn full_detector_getters_preserve_all_fields_and_are_total() {
        unsafe {
            let mut reasons = std::collections::BTreeMap::new();
            reasons.insert(2, vec!["second".to_string()]);
            reasons.insert(1, vec!["a\0b".to_string(), String::new()]);
            let result = Box::into_raw(Box::new(c_full_detection_result(crate::PdfTypeResult {
                pdf_type: crate::PdfType::Mixed,
                page_count: 3,
                pages_sampled: 2,
                pages_with_text: 1,
                confidence: 0.75,
                title: Some("title\0bytes".to_string()),
                ocr_recommended: true,
                pages_needing_ocr: vec![1, 2],
                ocr_reasons_by_page: reasons,
            })));

            assert_eq!(
                pdf_inspector_pdf_type_result_get_type(result),
                CPdfType::Mixed
            );
            assert_eq!(pdf_inspector_pdf_type_result_get_page_count(result), 3);
            assert_eq!(pdf_inspector_pdf_type_result_get_pages_sampled(result), 2);
            assert_eq!(pdf_inspector_pdf_type_result_get_pages_with_text(result), 1);
            assert_eq!(pdf_inspector_pdf_type_result_get_confidence(result), 0.75);
            assert!(pdf_inspector_pdf_type_result_is_ocr_recommended(result));

            let title =
                get_byte_view(|out| pdf_inspector_pdf_type_result_get_title(result, out)).unwrap();
            assert_eq!(
                std::slice::from_raw_parts(title.ptr, title.len),
                b"title\0bytes"
            );
            let pages = get_u32_view(|out| {
                pdf_inspector_pdf_type_result_get_pages_needing_ocr(result, out)
            })
            .unwrap();
            assert_eq!(std::slice::from_raw_parts(pages.ptr, pages.len), &[1, 2]);

            assert_eq!(
                pdf_inspector_pdf_type_result_get_ocr_reasons_count(result),
                2
            );
            assert_eq!(
                pdf_inspector_pdf_type_result_get_ocr_reasons_page(result, 0),
                1
            );
            assert_eq!(
                pdf_inspector_pdf_type_result_get_ocr_reasons_reasons_count(result, 0),
                2
            );
            let reason = get_byte_view(|out| {
                pdf_inspector_pdf_type_result_get_ocr_reasons_reason(result, 0, 0, out)
            })
            .unwrap();
            assert_eq!(std::slice::from_raw_parts(reason.ptr, reason.len), b"a\0b");
            assert_eq!(
                pdf_inspector_pdf_type_result_get_ocr_reasons_page(result, 2),
                0
            );

            let mut view = CByteView {
                ptr: std::ptr::NonNull::dangling().as_ptr(),
                len: 1,
            };
            assert!(!pdf_inspector_pdf_type_result_get_ocr_reasons_reason(
                result, 0, 2, &mut view,
            ));
            assert!(view.ptr.is_null());
            assert_eq!(view.len, 0);
            assert_eq!(
                pdf_inspector_pdf_type_result_get_type(std::ptr::null()),
                CPdfType::Unknown
            );
            assert_eq!(
                pdf_inspector_pdf_type_result_get_page_count(std::ptr::null()),
                0
            );

            pdf_inspector_pdf_type_result_free(result);
            pdf_inspector_pdf_type_result_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn test_c_api_advanced_getters() {
        unsafe {
            let options = pdf_inspector_options_new();
            assert!(!options.is_null());

            // Test options configuration
            for (name, set) in BOOL_SETTERS {
                assert_eq!(set(options, false), PdfInspectorError::Success, "{name}");
                assert_eq!(set(options, true), PdfInspectorError::Success, "{name}");
            }
            assert_eq!(
                pdf_inspector_options_set_profile(options, 1),
                PdfInspectorError::Success
            ); // Compact
            assert_eq!(
                pdf_inspector_options_set_min_text_ops_per_page(options, 5),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_options_set_text_page_ratio_threshold(options, 0.5),
                PdfInspectorError::Success
            );

            let path = CString::new(FIXTURE).unwrap();
            let mut result_ptr = std::ptr::null_mut();

            let err_proc = pdf_inspector_process_pdf(path.as_ptr(), options, &mut result_ptr);
            assert_eq!(err_proc, PdfInspectorError::Success);
            assert!(!result_ptr.is_null());

            // Test zero-copy string getters
            let markdown =
                get_byte_view(|out| pdf_inspector_result_get_markdown(result_ptr, out)).unwrap();
            assert!(!markdown.ptr.is_null());
            assert!(markdown.len > 0);

            // Test zero-copy arrays
            let ocr_pages =
                get_u32_view(|out| pdf_inspector_result_get_pages_needing_ocr(result_ptr, out))
                    .unwrap();
            assert_eq!(ocr_pages.len, 0);

            // Test detailed OCR reasons getters
            let reasons_count = pdf_inspector_result_get_ocr_reasons_count(result_ptr);
            assert_eq!(reasons_count, 0); // No ocr reasons for this text-based PDF

            pdf_inspector_result_free(result_ptr);
            pdf_inspector_options_free(options);
        }
    }

    #[test]
    fn null_handles_yield_zero_values() {
        unsafe {
            assert_eq!(
                pdf_inspector_result_get_type(std::ptr::null()),
                CPdfType::Unknown
            );
            assert_eq!(pdf_inspector_result_get_page_count(std::ptr::null()), 0);
            let mut byte_view = CByteView {
                ptr: std::ptr::NonNull::dangling().as_ptr(),
                len: 7,
            };
            assert!(!pdf_inspector_result_get_markdown(
                std::ptr::null(),
                &mut byte_view
            ));
            assert!(byte_view.ptr.is_null());
            assert_eq!(byte_view.len, 0);
            assert_eq!(
                pdf_inspector_classification_get_type(std::ptr::null()),
                CPdfType::Unknown
            );
            assert_eq!(
                pdf_inspector_pages_result_get_page_count(std::ptr::null()),
                0
            );
            assert!(!pdf_inspector_pages_result_get_page_markdown(
                std::ptr::null(),
                0,
                &mut byte_view,
            ));
            assert!(byte_view.ptr.is_null());
            assert_eq!(byte_view.len, 0);
            assert_eq!(
                pdf_inspector_text_items_result_get_count(std::ptr::null()),
                0
            );
            assert_eq!(
                pdf_inspector_text_items_result_get_type(std::ptr::null(), 0),
                CTextItemType::Unknown
            );
            assert_eq!(
                pdf_inspector_structure_elements_result_get_count(std::ptr::null()),
                0
            );

            // A NULL handle still zeroes the output view and returns false.
            let mut pages_view = CU32View {
                ptr: std::ptr::NonNull::dangling().as_ptr(),
                len: 7,
            };
            assert!(!pdf_inspector_result_get_pages_needing_ocr(
                std::ptr::null(),
                &mut pages_view,
            ));
            assert!(pages_view.ptr.is_null());
            assert_eq!(pages_view.len, 0);
            // A NULL output is tolerated.
            assert!(!pdf_inspector_result_get_pages_needing_ocr(
                std::ptr::null(),
                std::ptr::null_mut(),
            ));
        }
    }

    #[test]
    fn borrowed_strings_are_lossless_and_distinguish_absence() {
        unsafe {
            let result = CPdfProcessResult {
                inner: crate::PdfProcessResult {
                    pdf_type: crate::PdfType::TextBased,
                    markdown: Some("a\0b".into()),
                    page_count: 1,
                    processing_time_ms: 0,
                    pages_needing_ocr: vec![1],
                    ocr_reasons_by_page: vec![crate::PageOcrReasons {
                        page: 1,
                        reasons: vec!["\0".into(), "".into()],
                    }],
                    title: Some(String::new()),
                    confidence: 1.0,
                    layout: crate::LayoutComplexity::default(),
                    has_encoding_issues: false,
                },
            };
            let result = Box::into_raw(Box::new(result));

            let markdown =
                get_byte_view(|out| pdf_inspector_result_get_markdown(result, out)).unwrap();
            assert_eq!(
                std::slice::from_raw_parts(markdown.ptr, markdown.len),
                b"a\0b"
            );
            let title = get_byte_view(|out| pdf_inspector_result_get_title(result, out)).unwrap();
            assert!(!title.ptr.is_null());
            assert_eq!(title.len, 0);
            let reason =
                get_byte_view(|out| pdf_inspector_result_get_ocr_reasons_reason(result, 0, 0, out))
                    .unwrap();
            assert_eq!(std::slice::from_raw_parts(reason.ptr, reason.len), b"\0");
            let empty_reason =
                get_byte_view(|out| pdf_inspector_result_get_ocr_reasons_reason(result, 0, 1, out))
                    .unwrap();
            assert!(!empty_reason.ptr.is_null());
            assert_eq!(empty_reason.len, 0);
            assert!(get_byte_view(|out| {
                pdf_inspector_result_get_ocr_reasons_reason(result, 1, 0, out)
            })
            .is_none());
            (*result).inner.title = None;
            assert!(get_byte_view(|out| pdf_inspector_result_get_title(result, out)).is_none());

            pdf_inspector_result_free(result);
        }
    }

    #[test]
    fn positioned_text_and_structure_getters_preserve_join_fields() {
        unsafe {
            let text_items = Box::into_raw(Box::new(CTextItemsResult {
                items: vec![crate::TextItem {
                    text: "heading".into(),
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                    font: "TestFont".into(),
                    font_size: 12.0,
                    page: 1,
                    is_bold: true,
                    is_italic: false,
                    is_underline: true,
                    is_strikeout: false,
                    item_type: crate::types::ItemType::Link("https://example.com".into()),
                    mcid: Some(7),
                }],
            }));
            let elements = Box::into_raw(Box::new(CStructureElementsResult {
                elements: vec![crate::StructureElement {
                    page: 1,
                    mcid: 7,
                    role: "H1".into(),
                }],
            }));

            assert_eq!(pdf_inspector_text_items_result_get_count(text_items), 1);
            assert_eq!(pdf_inspector_text_items_result_get_page(text_items, 0), 1);
            assert_eq!(
                pdf_inspector_text_items_result_get_type(text_items, 0),
                CTextItemType::Link
            );
            assert!(pdf_inspector_text_items_result_is_bold(text_items, 0));
            assert!(pdf_inspector_text_items_result_is_underline(text_items, 0));
            assert!(pdf_inspector_text_items_result_has_mcid(text_items, 0));
            assert_eq!(pdf_inspector_text_items_result_get_mcid(text_items, 0), 7);
            let link = get_byte_view(|out| {
                pdf_inspector_text_items_result_get_link_url(text_items, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(link.ptr, link.len),
                b"https://example.com"
            );
            assert_eq!(
                pdf_inspector_structure_elements_result_get_count(elements),
                1
            );
            assert_eq!(
                pdf_inspector_structure_elements_result_get_page(elements, 0),
                1
            );
            assert_eq!(
                pdf_inspector_structure_elements_result_get_mcid(elements, 0),
                7
            );
            let role = get_byte_view(|out| {
                pdf_inspector_structure_elements_result_get_role(elements, 0, out)
            })
            .unwrap();
            assert_eq!(std::slice::from_raw_parts(role.ptr, role.len), b"H1");

            pdf_inspector_text_items_result_free(text_items);
            pdf_inspector_structure_elements_result_free(elements);
        }
    }

    #[test]
    fn region_text_getters_preserve_shape_text_and_ocr_metadata() {
        unsafe {
            let result = Box::into_raw(Box::new(CRegionTextResult {
                pages: vec![crate::PageRegionResult {
                    page: 2,
                    regions: vec![
                        crate::RegionText {
                            text: "a\0b".into(),
                            needs_ocr: false,
                            ocr_reason: None,
                        },
                        crate::RegionText {
                            text: String::new(),
                            needs_ocr: true,
                            ocr_reason: Some("empty".into()),
                        },
                    ],
                }],
            }));

            assert_eq!(pdf_inspector_region_text_result_get_page_count(result), 1);
            assert_eq!(pdf_inspector_region_text_result_get_page(result, 0), 3);
            assert_eq!(
                pdf_inspector_region_text_result_get_region_count(result, 0),
                2
            );
            let text =
                get_byte_view(|out| pdf_inspector_region_text_result_get_text(result, 0, 0, out))
                    .unwrap();
            assert_eq!(std::slice::from_raw_parts(text.ptr, text.len), b"a\0b");
            assert!(!pdf_inspector_region_text_result_needs_ocr(result, 0, 0));
            assert!(get_byte_view(|out| {
                pdf_inspector_region_text_result_get_ocr_reason(result, 0, 0, out)
            })
            .is_none());
            assert!(pdf_inspector_region_text_result_needs_ocr(result, 0, 1));
            let empty_text =
                get_byte_view(|out| pdf_inspector_region_text_result_get_text(result, 0, 1, out))
                    .unwrap();
            assert!(!empty_text.ptr.is_null());
            assert_eq!(empty_text.len, 0);
            let reason = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_ocr_reason(result, 0, 1, out)
            })
            .unwrap();
            assert_eq!(reason.len, 5);

            assert_eq!(pdf_inspector_region_text_result_get_page(result, 1), 0);
            assert!(get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(result, 0, 2, out)
            })
            .is_none());
            assert_eq!(
                pdf_inspector_region_text_result_get_page_count(std::ptr::null()),
                0
            );
            pdf_inspector_region_text_result_free(result);
            pdf_inspector_region_text_result_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn region_text_ffi_extracts_path_and_bytes_and_validates_descriptors() {
        unsafe {
            let path = CString::new(FIXTURE).unwrap();
            let buffer = std::fs::read(FIXTURE).unwrap();
            let regions = [CRegion {
                x1: 0.0,
                y1: 0.0,
                x2: 2_000.0,
                y2: 2_000.0,
            }];
            let page_regions = [CPageRegions {
                page: 1,
                regions: regions.as_ptr(),
                regions_count: regions.len(),
            }];

            let mut path_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_in_regions(
                    path.as_ptr(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    std::ptr::null(),
                    &mut path_result,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_region_text_result_get_page(path_result, 0), 1);
            assert_eq!(
                pdf_inspector_region_text_result_get_region_count(path_result, 0),
                1
            );
            let path_text = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(path_result, 0, 0, out)
            })
            .unwrap();
            assert!(path_text.len > 0);

            let mut mem_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    std::ptr::null(),
                    &mut mem_result,
                ),
                PdfInspectorError::Success
            );
            let mem_text = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(mem_result, 0, 0, out)
            })
            .unwrap();
            assert_eq!(mem_text.len, path_text.len);

            let invalid_page = [CPageRegions {
                page: 0,
                regions: regions.as_ptr(),
                regions_count: regions.len(),
            }];
            let mut invalid_result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_text_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    invalid_page.as_ptr(),
                    invalid_page.len(),
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_result.is_null());

            let invalid_regions = [CPageRegions {
                page: 1,
                regions: std::ptr::null(),
                regions_count: 1,
            }];
            assert_eq!(
                pdf_inspector_extract_text_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    invalid_regions.as_ptr(),
                    invalid_regions.len(),
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_result.is_null());

            let non_finite = [CRegion {
                x1: f32::NAN,
                y1: 0.0,
                x2: 1.0,
                y2: 1.0,
            }];
            let invalid_coordinates = [CPageRegions {
                page: 1,
                regions: non_finite.as_ptr(),
                regions_count: non_finite.len(),
            }];
            assert_eq!(
                pdf_inspector_extract_text_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    invalid_coordinates.as_ptr(),
                    invalid_coordinates.len(),
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );

            pdf_inspector_region_text_result_free(path_result);
            pdf_inspector_region_text_result_free(mem_result);
        }
    }

    #[test]
    fn table_region_ffi_extracts_path_and_bytes_with_password_support() {
        unsafe {
            const TABLE_FIXTURE: &str = "tests/fixtures/tnagriculture_06_12.pdf";
            let path = CString::new(TABLE_FIXTURE).unwrap();
            let buffer = std::fs::read(TABLE_FIXTURE).unwrap();
            let regions = [CRegion {
                x1: 0.0,
                y1: 0.0,
                x2: 1_200.0,
                y2: 1_200.0,
            }];
            let page_regions = [CPageRegions {
                page: 1,
                regions: regions.as_ptr(),
                regions_count: regions.len(),
            }];

            let mut path_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_in_regions(
                    path.as_ptr(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    std::ptr::null(),
                    &mut path_result,
                ),
                PdfInspectorError::Success
            );
            assert!(!pdf_inspector_region_text_result_needs_ocr(
                path_result,
                0,
                0
            ));
            let path_table = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(path_result, 0, 0, out)
            })
            .unwrap();
            assert!(std::slice::from_raw_parts(path_table.ptr, path_table.len).contains(&b'|'));

            let mut mem_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    std::ptr::null(),
                    &mut mem_result,
                ),
                PdfInspectorError::Success
            );
            let mem_table = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(mem_result, 0, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(mem_table.ptr, mem_table.len),
                std::slice::from_raw_parts(path_table.ptr, path_table.len)
            );

            let reversed_regions = [CRegion {
                x1: 1_200.0,
                y1: 1_200.0,
                x2: 0.0,
                y2: 0.0,
            }];
            let reversed_page_regions = [CPageRegions {
                page: 1,
                regions: reversed_regions.as_ptr(),
                regions_count: reversed_regions.len(),
            }];
            let mut reversed_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    reversed_page_regions.as_ptr(),
                    reversed_page_regions.len(),
                    std::ptr::null(),
                    &mut reversed_result,
                ),
                PdfInspectorError::Success
            );
            let reversed_table = get_byte_view(|out| {
                pdf_inspector_region_text_result_get_text(reversed_result, 0, 0, out)
            })
            .unwrap();
            assert_eq!(
                pdf_inspector_region_text_result_needs_ocr(reversed_result, 0, 0),
                pdf_inspector_region_text_result_needs_ocr(mem_result, 0, 0)
            );
            assert_eq!(
                std::slice::from_raw_parts(reversed_table.ptr, reversed_table.len),
                std::slice::from_raw_parts(mem_table.ptr, mem_table.len)
            );

            let encrypted = std::fs::read("tests/fixtures/encrypted-secret123.pdf").unwrap();
            let password = CString::new("secret123").unwrap();
            let wrong_password = CString::new("wrong").unwrap();
            let mut encrypted_result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_tables_in_regions_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    wrong_password.as_ptr(),
                    &mut encrypted_result,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(encrypted_result.is_null());
            assert_eq!(
                pdf_inspector_extract_tables_in_regions_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    password.as_ptr(),
                    &mut encrypted_result,
                ),
                PdfInspectorError::Success
            );

            pdf_inspector_region_text_result_free(path_result);
            pdf_inspector_region_text_result_free(mem_result);
            pdf_inspector_region_text_result_free(reversed_result);
            pdf_inspector_region_text_result_free(encrypted_result);
        }
    }

    #[test]
    fn tsr_auto_ffi_supports_path_bytes_password_and_validates_descriptors() {
        unsafe {
            const TSR_FIXTURE: &str = "tests/fixtures/bits_pilani_feedback.pdf";
            let path = CString::new(TSR_FIXTURE).unwrap();
            let buffer = std::fs::read(TSR_FIXTURE).unwrap();
            let token_bytes: [&[u8]; 14] = [
                b"<table>",
                b"<thead>",
                b"<tr>",
                b"<th></th>",
                b"<th></th>",
                b"</tr>",
                b"</thead>",
                b"<tbody>",
                b"<tr>",
                b"<td></td>",
                b"<td></td>",
                b"</tr>",
                b"</tbody>",
                b"</table>",
            ];
            let tokens: Vec<CByteView> = token_bytes
                .iter()
                .map(|token| CByteView {
                    ptr: token.as_ptr(),
                    len: token.len(),
                })
                .collect();
            let box_coordinates = [
                [10.0, 7.0, 100.0, 18.0],
                [110.0, 7.0, 200.0, 18.0],
                [10.0, 35.0, 100.0, 60.0],
                [110.0, 35.0, 200.0, 60.0],
            ];
            let boxes: Vec<CTsrCellBBox> = box_coordinates
                .iter()
                .map(|coordinates| CTsrCellBBox {
                    coordinates: coordinates.as_ptr(),
                    coordinates_count: coordinates.len(),
                })
                .collect();
            let input = CTsrTableInput {
                page: 4,
                crop_pdf_pt_bbox: CRegion {
                    x1: 80.0,
                    y1: 170.0,
                    x2: 280.0,
                    y2: 240.0,
                },
                render_dpi: 72.0,
                structure_tokens: tokens.as_ptr(),
                structure_tokens_count: tokens.len(),
                cell_bboxes: boxes.as_ptr(),
                cell_bboxes_count: boxes.len(),
            };

            let mut path_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto(
                    path.as_ptr(),
                    &input,
                    1,
                    std::ptr::null(),
                    &mut path_result,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_tsr_result_get_count(path_result), 1);
            let path_markdown =
                get_byte_view(|out| pdf_inspector_tsr_result_get_markdown(path_result, 0, out))
                    .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(path_markdown.ptr, path_markdown.len),
                b"|Department|Core Courses|\n|---|---|\n|BIO|8.23|\n"
            );
            let mut absent_reason = CByteView {
                ptr: std::ptr::NonNull::dangling().as_ptr(),
                len: 99,
            };
            assert!(!pdf_inspector_tsr_result_get_fallback_reason(
                path_result,
                0,
                &mut absent_reason,
            ));
            assert!(absent_reason.ptr.is_null());
            assert_eq!(absent_reason.len, 0);

            let mut mem_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    &input,
                    1,
                    std::ptr::null(),
                    &mut mem_result,
                ),
                PdfInspectorError::Success
            );
            let mem_markdown =
                get_byte_view(|out| pdf_inspector_tsr_result_get_markdown(mem_result, 0, out))
                    .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(mem_markdown.ptr, mem_markdown.len),
                std::slice::from_raw_parts(path_markdown.ptr, path_markdown.len)
            );

            let mut path_cells = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_cells(
                    path.as_ptr(),
                    &input,
                    1,
                    std::ptr::null(),
                    &mut path_cells,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_table_count(path_cells),
                1
            );
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_cell_count(path_cells, 0),
                4
            );
            let mut first_cell = CTsrStructuredCell::default();
            assert!(pdf_inspector_tsr_cells_result_get_cell(
                path_cells,
                0,
                0,
                &mut first_cell,
            ));
            assert_eq!(first_cell.row, 0);
            assert_eq!(first_cell.col, 0);
            assert_eq!(first_cell.rowspan, 1);
            assert_eq!(first_cell.colspan, 1);
            assert!(first_cell.is_header);
            assert_eq!(
                first_cell.page_pt_bbox,
                CRegion {
                    x1: 90.0,
                    y1: 177.0,
                    x2: 180.0,
                    y2: 188.0,
                }
            );
            let first_text = get_byte_view(|out| {
                pdf_inspector_tsr_cells_result_get_cell_text(path_cells, 0, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(first_text.ptr, first_text.len),
                b"Department"
            );

            let mut mem_cells = std::ptr::null_mut();
            let cell_inputs = [
                input,
                CTsrTableInput {
                    page: 10_000,
                    ..input
                },
            ];
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_cells_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    cell_inputs.as_ptr(),
                    cell_inputs.len(),
                    std::ptr::null(),
                    &mut mem_cells,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_tsr_cells_result_get_table_count(mem_cells), 2);
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_cell_count(mem_cells, 1),
                0
            );
            let last_text = get_byte_view(|out| {
                pdf_inspector_tsr_cells_result_get_cell_text(mem_cells, 0, 3, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(last_text.ptr, last_text.len),
                b"8.23"
            );
            let mut missing_cell = CTsrStructuredCell {
                row: 99,
                ..CTsrStructuredCell::default()
            };
            assert!(!pdf_inspector_tsr_cells_result_get_cell(
                mem_cells,
                0,
                4,
                &mut missing_cell,
            ));
            assert_eq!(missing_cell, CTsrStructuredCell::default());
            let mut missing_text = CByteView {
                ptr: std::ptr::NonNull::dangling().as_ptr(),
                len: 99,
            };
            assert!(!pdf_inspector_tsr_cells_result_get_cell_text(
                mem_cells,
                2,
                0,
                &mut missing_text,
            ));
            assert!(missing_text.ptr.is_null());
            assert_eq!(missing_text.len, 0);

            let mut empty_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut empty_result,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_tsr_result_get_count(empty_result), 0);

            let mut invalid_result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    std::ptr::NonNull::<u8>::dangling().as_ptr(),
                    (isize::MAX as usize).saturating_add(1),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_result.is_null());
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_result.is_null());
            for invalid_input in [
                CTsrTableInput { page: 0, ..input },
                CTsrTableInput {
                    render_dpi: f32::NAN,
                    ..input
                },
                CTsrTableInput {
                    crop_pdf_pt_bbox: CRegion {
                        x1: 280.0,
                        y1: 170.0,
                        x2: 80.0,
                        y2: 240.0,
                    },
                    ..input
                },
                CTsrTableInput {
                    cell_bboxes_count: boxes.len() - 1,
                    ..input
                },
            ] {
                assert_eq!(
                    pdf_inspector_extract_tables_with_structure_auto_mem(
                        buffer.as_ptr(),
                        buffer.len(),
                        &invalid_input,
                        1,
                        std::ptr::null(),
                        &mut invalid_result,
                    ),
                    PdfInspectorError::InvalidArgument
                );
                assert!(invalid_result.is_null());
            }
            let malformed_box = CTsrCellBBox {
                coordinates: box_coordinates[0].as_ptr(),
                coordinates_count: 3,
            };
            let malformed_input = CTsrTableInput {
                cell_bboxes: &malformed_box,
                cell_bboxes_count: 1,
                structure_tokens_count: 4,
                ..input
            };
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    &malformed_input,
                    1,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );

            let huge_span_bytes: [&[u8]; 5] = [
                b"<table>",
                b"<tr>",
                b"<td",
                b" colspan=\"18446744073709551615\"",
                b">",
            ];
            let huge_span_tokens: Vec<CByteView> = huge_span_bytes
                .iter()
                .map(|token| CByteView {
                    ptr: token.as_ptr(),
                    len: token.len(),
                })
                .collect();
            let huge_span_input = CTsrTableInput {
                structure_tokens: huge_span_tokens.as_ptr(),
                structure_tokens_count: huge_span_tokens.len(),
                cell_bboxes: boxes.as_ptr(),
                cell_bboxes_count: 1,
                ..input
            };
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    &huge_span_input,
                    1,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );

            let encrypted = std::fs::read("tests/fixtures/encrypted-secret123.pdf").unwrap();
            let encrypted_path = CString::new("tests/fixtures/encrypted-secret123.pdf").unwrap();
            let password = CString::new("secret123").unwrap();
            let wrong_password = CString::new("wrong").unwrap();
            let encrypted_token_bytes: [&[u8]; 11] = [
                b"<table>",
                b"<tr>",
                b"<td></td>",
                b"</tr>",
                b"<tr>",
                b"<td></td>",
                b"</tr>",
                b"<tr>",
                b"<td></td>",
                b"</tr>",
                b"</table>",
            ];
            let encrypted_tokens: Vec<CByteView> = encrypted_token_bytes
                .iter()
                .map(|token| CByteView {
                    ptr: token.as_ptr(),
                    len: token.len(),
                })
                .collect();
            let encrypted_coordinates = [
                [0.0, 0.0, 1000.0, 200.0],
                [0.0, 700.0, 1000.0, 710.0],
                [0.0, 200.0, 1000.0, 600.0],
            ];
            let encrypted_boxes: Vec<CTsrCellBBox> = encrypted_coordinates
                .iter()
                .map(|coordinates| CTsrCellBBox {
                    coordinates: coordinates.as_ptr(),
                    coordinates_count: coordinates.len(),
                })
                .collect();
            let encrypted_input = CTsrTableInput {
                page: 1,
                crop_pdf_pt_bbox: CRegion {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 1000.0,
                    y2: 800.0,
                },
                render_dpi: 72.0,
                structure_tokens: encrypted_tokens.as_ptr(),
                structure_tokens_count: encrypted_tokens.len(),
                cell_bboxes: encrypted_boxes.as_ptr(),
                cell_bboxes_count: encrypted_boxes.len(),
            };
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    &encrypted_input,
                    1,
                    wrong_password.as_ptr(),
                    &mut invalid_result,
                ),
                PdfInspectorError::Encrypted
            );
            let mut encrypted_cells = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_cells_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    &encrypted_input,
                    1,
                    wrong_password.as_ptr(),
                    &mut encrypted_cells,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(encrypted_cells.is_null());
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_cells_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    &encrypted_input,
                    1,
                    password.as_ptr(),
                    &mut encrypted_cells,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_cell_count(encrypted_cells, 0),
                3
            );
            let mut encrypted_path_cells = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_cells(
                    encrypted_path.as_ptr(),
                    &encrypted_input,
                    1,
                    password.as_ptr(),
                    &mut encrypted_path_cells,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_cell_count(encrypted_path_cells, 0),
                3
            );
            let mut encrypted_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto(
                    encrypted_path.as_ptr(),
                    &encrypted_input,
                    1,
                    password.as_ptr(),
                    &mut encrypted_result,
                ),
                PdfInspectorError::Success
            );
            let encrypted_markdown = get_byte_view(|out| {
                pdf_inspector_tsr_result_get_markdown(encrypted_result, 0, out)
            })
            .unwrap();
            let mut reason = CByteView::default();
            assert!(pdf_inspector_tsr_result_get_fallback_reason(
                encrypted_result,
                0,
                &mut reason,
            ));
            let reason_bytes = std::slice::from_raw_parts(reason.ptr, reason.len);
            assert!(reason_bytes.starts_with(b"phantom_empty_row"));
            assert!(!reason_bytes
                .windows(b"error".len())
                .any(|window| window == b"error"));

            let mut encrypted_mem_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_tables_with_structure_auto_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    &encrypted_input,
                    1,
                    password.as_ptr(),
                    &mut encrypted_mem_result,
                ),
                PdfInspectorError::Success
            );
            let encrypted_mem_markdown = get_byte_view(|out| {
                pdf_inspector_tsr_result_get_markdown(encrypted_mem_result, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(encrypted_mem_markdown.ptr, encrypted_mem_markdown.len),
                std::slice::from_raw_parts(encrypted_markdown.ptr, encrypted_markdown.len)
            );
            let encrypted_mem_reason = get_byte_view(|out| {
                pdf_inspector_tsr_result_get_fallback_reason(encrypted_mem_result, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(encrypted_mem_reason.ptr, encrypted_mem_reason.len),
                reason_bytes
            );

            pdf_inspector_tsr_result_free(path_result);
            pdf_inspector_tsr_result_free(mem_result);
            pdf_inspector_tsr_result_free(empty_result);
            pdf_inspector_tsr_result_free(encrypted_result);
            pdf_inspector_tsr_result_free(encrypted_mem_result);
            pdf_inspector_tsr_result_free(std::ptr::null_mut());
            pdf_inspector_tsr_cells_result_free(path_cells);
            pdf_inspector_tsr_cells_result_free(mem_cells);
            pdf_inspector_tsr_cells_result_free(encrypted_cells);
            pdf_inspector_tsr_cells_result_free(encrypted_path_cells);
            pdf_inspector_tsr_cells_result_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn tsr_result_getters_preserve_empty_and_embedded_nul_strings() {
        unsafe {
            let result = Box::into_raw(Box::new(CTsrTableExtractionResult {
                results: vec![
                    crate::TableExtractionResult {
                        markdown: "a\0b".to_string(),
                        fallback_reason: None,
                    },
                    crate::TableExtractionResult {
                        markdown: String::new(),
                        fallback_reason: Some("phantom_empty_row".to_string()),
                    },
                ],
            }));
            assert_eq!(pdf_inspector_tsr_result_get_count(result), 2);
            let first =
                get_byte_view(|out| pdf_inspector_tsr_result_get_markdown(result, 0, out)).unwrap();
            assert_eq!(std::slice::from_raw_parts(first.ptr, first.len), b"a\0b");
            let empty =
                get_byte_view(|out| pdf_inspector_tsr_result_get_markdown(result, 1, out)).unwrap();
            assert!(!empty.ptr.is_null());
            assert_eq!(empty.len, 0);
            let reason =
                get_byte_view(|out| pdf_inspector_tsr_result_get_fallback_reason(result, 1, out))
                    .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(reason.ptr, reason.len),
                b"phantom_empty_row"
            );
            assert!(
                get_byte_view(|out| { pdf_inspector_tsr_result_get_markdown(result, 2, out) })
                    .is_none()
            );
            assert_eq!(pdf_inspector_tsr_result_get_count(std::ptr::null()), 0);
            pdf_inspector_tsr_result_free(result);
        }
    }

    #[test]
    fn tsr_cell_getters_preserve_empty_and_embedded_nul_text() {
        unsafe {
            let result = Box::into_raw(Box::new(CTsrStructuredCellsResult {
                tables: vec![vec![
                    crate::tables::StructuredCell {
                        row: 2,
                        col: 3,
                        rowspan: 1,
                        colspan: 2,
                        is_header: false,
                        text: "a\0b".to_string(),
                        page_pt_bbox: [1.0, 2.0, 3.0, 4.0],
                    },
                    crate::tables::StructuredCell {
                        row: 3,
                        col: 0,
                        rowspan: 1,
                        colspan: 1,
                        is_header: true,
                        text: String::new(),
                        page_pt_bbox: [5.0, 6.0, 7.0, 8.0],
                    },
                ]],
            }));
            assert_eq!(pdf_inspector_tsr_cells_result_get_table_count(result), 1);
            assert_eq!(pdf_inspector_tsr_cells_result_get_cell_count(result, 0), 2);
            let mut cell = CTsrStructuredCell::default();
            assert!(pdf_inspector_tsr_cells_result_get_cell(
                result, 0, 0, &mut cell,
            ));
            assert_eq!(cell.row, 2);
            assert_eq!(cell.col, 3);
            assert_eq!(cell.colspan, 2);
            assert_eq!(cell.page_pt_bbox.x1, 1.0);
            let text = get_byte_view(|out| {
                pdf_inspector_tsr_cells_result_get_cell_text(result, 0, 0, out)
            })
            .unwrap();
            assert_eq!(std::slice::from_raw_parts(text.ptr, text.len), b"a\0b");
            let empty = get_byte_view(|out| {
                pdf_inspector_tsr_cells_result_get_cell_text(result, 0, 1, out)
            })
            .unwrap();
            assert!(!empty.ptr.is_null());
            assert_eq!(empty.len, 0);
            assert_eq!(
                pdf_inspector_tsr_cells_result_get_cell_count(std::ptr::null(), 0),
                0
            );
            pdf_inspector_tsr_cells_result_free(result);
        }
    }

    #[test]
    fn descriptor_converters_reject_oversized_page_and_region_arrays() {
        unsafe {
            let oversized_u32_count = (isize::MAX as usize / std::mem::size_of::<u32>()) + 1;
            assert_eq!(
                pages_from_ffi(
                    std::ptr::NonNull::<u32>::dangling().as_ptr(),
                    oversized_u32_count,
                ),
                Err(PdfInspectorError::InvalidArgument)
            );

            let oversized_page_count =
                (isize::MAX as usize / std::mem::size_of::<CPageRegions>()) + 1;
            assert_eq!(
                page_regions_from_ffi(
                    std::ptr::NonNull::<CPageRegions>::dangling().as_ptr(),
                    oversized_page_count,
                ),
                Err(PdfInspectorError::InvalidArgument)
            );

            let page_regions = CPageRegions {
                page: 1,
                regions: std::ptr::NonNull::<CRegion>::dangling().as_ptr(),
                regions_count: (isize::MAX as usize / std::mem::size_of::<CRegion>()) + 1,
            };
            assert_eq!(
                page_regions_from_ffi(&page_regions, 1),
                Err(PdfInspectorError::InvalidArgument)
            );
        }
    }

    #[test]
    fn vector_grid_ffi_detects_path_and_bytes_and_validates_inputs() {
        unsafe {
            const GRID_FIXTURE: &str = "tests/fixtures/multiline_indent_cell_rect_grid.pdf";
            let path = CString::new(GRID_FIXTURE).unwrap();
            let buffer = std::fs::read(GRID_FIXTURE).unwrap();
            let region = CRegion {
                x1: 0.0,
                y1: 0.0,
                x2: 612.0,
                y2: 792.0,
            };

            let mut path_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region(
                    path.as_ptr(),
                    30,
                    &region,
                    200.0,
                    std::ptr::null(),
                    &mut path_result,
                ),
                PdfInspectorError::Success
            );
            assert!(pdf_inspector_vector_grid_result_is_detected(path_result));
            let token_count =
                pdf_inspector_vector_grid_result_get_structure_token_count(path_result);
            let cell_count = pdf_inspector_vector_grid_result_get_cell_count(path_result);
            assert!(token_count > 0);
            assert!(cell_count >= 15);
            let first_token = get_byte_view(|out| {
                pdf_inspector_vector_grid_result_get_structure_token(path_result, 0, out)
            })
            .unwrap();
            assert_eq!(
                std::slice::from_raw_parts(first_token.ptr, first_token.len),
                b"<table>"
            );
            let mut first_box = CVectorGridCellBox::default();
            assert!(pdf_inspector_vector_grid_result_get_cell_box(
                path_result,
                0,
                &mut first_box,
            ));
            assert!(first_box.x1.is_finite());
            assert!(first_box.y1.is_finite());
            assert!(first_box.x2 > first_box.x1);
            assert!(first_box.y2 > first_box.y1);

            let mut mem_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    30,
                    &region,
                    200.0,
                    std::ptr::null(),
                    &mut mem_result,
                ),
                PdfInspectorError::Success
            );
            assert!(pdf_inspector_vector_grid_result_is_detected(mem_result));
            assert_eq!(
                pdf_inspector_vector_grid_result_get_structure_token_count(mem_result),
                token_count
            );
            assert_eq!(
                pdf_inspector_vector_grid_result_get_cell_count(mem_result),
                cell_count
            );
            let mut mem_first_box = CVectorGridCellBox::default();
            assert!(pdf_inspector_vector_grid_result_get_cell_box(
                mem_result,
                0,
                &mut mem_first_box,
            ));
            assert_eq!(mem_first_box, first_box);

            let reversed_region = CRegion {
                x1: region.x2,
                y1: region.y2,
                x2: region.x1,
                y2: region.y1,
            };
            let mut reversed_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    30,
                    &reversed_region,
                    200.0,
                    std::ptr::null(),
                    &mut reversed_result,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_vector_grid_result_get_structure_token_count(reversed_result),
                token_count
            );
            assert_eq!(
                pdf_inspector_vector_grid_result_get_cell_count(reversed_result),
                cell_count
            );
            let mut reversed_first_box = CVectorGridCellBox::default();
            assert!(pdf_inspector_vector_grid_result_get_cell_box(
                reversed_result,
                0,
                &mut reversed_first_box,
            ));
            assert_eq!(reversed_first_box, first_box);

            let mut no_grid_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    31,
                    &region,
                    200.0,
                    std::ptr::null(),
                    &mut no_grid_result,
                ),
                PdfInspectorError::Success
            );
            assert!(!no_grid_result.is_null());
            assert!(!pdf_inspector_vector_grid_result_is_detected(
                no_grid_result
            ));
            assert_eq!(
                pdf_inspector_vector_grid_result_get_structure_token_count(no_grid_result),
                0
            );
            assert_eq!(
                pdf_inspector_vector_grid_result_get_cell_count(no_grid_result),
                0
            );
            let mut missing_box = CVectorGridCellBox {
                x1: 1.0,
                y1: 1.0,
                x2: 1.0,
                y2: 1.0,
            };
            assert!(!pdf_inspector_vector_grid_result_get_cell_box(
                no_grid_result,
                0,
                &mut missing_box,
            ));
            assert_eq!(missing_box, CVectorGridCellBox::default());

            let mut invalid_result = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    0,
                    &region,
                    200.0,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_result.is_null());
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    1,
                    std::ptr::null(),
                    200.0,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::NullPointer
            );
            let non_finite_region = CRegion {
                x1: f32::NAN,
                ..region
            };
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    1,
                    &non_finite_region,
                    200.0,
                    std::ptr::null(),
                    &mut invalid_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            for invalid_dpi in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
                assert_eq!(
                    pdf_inspector_detect_vector_grid_in_region_mem(
                        buffer.as_ptr(),
                        buffer.len(),
                        1,
                        &region,
                        invalid_dpi,
                        std::ptr::null(),
                        &mut invalid_result,
                    ),
                    PdfInspectorError::InvalidArgument
                );
            }

            let encrypted = std::fs::read("tests/fixtures/encrypted-secret123.pdf").unwrap();
            let password = CString::new("secret123").unwrap();
            let wrong_password = CString::new("wrong").unwrap();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    1,
                    &region,
                    200.0,
                    wrong_password.as_ptr(),
                    &mut invalid_result,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(invalid_result.is_null());
            let mut encrypted_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_detect_vector_grid_in_region_mem(
                    encrypted.as_ptr(),
                    encrypted.len(),
                    1,
                    &region,
                    200.0,
                    password.as_ptr(),
                    &mut encrypted_result,
                ),
                PdfInspectorError::Success
            );

            assert!(!pdf_inspector_vector_grid_result_is_detected(
                std::ptr::null()
            ));
            assert_eq!(
                pdf_inspector_vector_grid_result_get_cell_count(std::ptr::null()),
                0
            );
            pdf_inspector_vector_grid_result_free(path_result);
            pdf_inspector_vector_grid_result_free(mem_result);
            pdf_inspector_vector_grid_result_free(reversed_result);
            pdf_inspector_vector_grid_result_free(no_grid_result);
            pdf_inspector_vector_grid_result_free(encrypted_result);
            pdf_inspector_vector_grid_result_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn tagged_pdf_ffi_supports_join_page_filters_and_bytes() {
        unsafe {
            let path = CString::new(TAGGED_FIXTURE).unwrap();
            let buffer = std::fs::read(TAGGED_FIXTURE).unwrap();
            let mut path_items = std::ptr::null_mut();
            let mut path_elements = std::ptr::null_mut();

            assert_eq!(
                pdf_inspector_extract_text_with_positions(
                    path.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut path_items,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_extract_structure_elements(
                    path.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut path_elements,
                ),
                PdfInspectorError::Success
            );
            assert!(!path_items.is_null());
            assert!(!path_elements.is_null());

            let item_count = pdf_inspector_text_items_result_get_count(path_items);
            let element_count = pdf_inspector_structure_elements_result_get_count(path_elements);
            assert!(item_count > 0);
            assert!(element_count > 0);

            let mut joined_h1_text = false;
            for element_index in 0..element_count {
                let role = get_byte_view(|out| {
                    pdf_inspector_structure_elements_result_get_role(
                        path_elements,
                        element_index,
                        out,
                    )
                })
                .unwrap();
                let role = std::slice::from_raw_parts(role.ptr, role.len);
                if role != b"H1" {
                    continue;
                }

                let page =
                    pdf_inspector_structure_elements_result_get_page(path_elements, element_index);
                let mcid =
                    pdf_inspector_structure_elements_result_get_mcid(path_elements, element_index);
                joined_h1_text = (0..item_count).any(|item_index| {
                    if pdf_inspector_text_items_result_get_page(path_items, item_index) != page
                        || !pdf_inspector_text_items_result_has_mcid(path_items, item_index)
                        || pdf_inspector_text_items_result_get_mcid(path_items, item_index) != mcid
                    {
                        return false;
                    }
                    let text = get_byte_view(|out| {
                        pdf_inspector_text_items_result_get_text(path_items, item_index, out)
                    })
                    .unwrap();
                    let text = std::slice::from_raw_parts(text.ptr, text.len);
                    text.iter().any(|byte| !byte.is_ascii_whitespace())
                });
                if joined_h1_text {
                    break;
                }
            }
            assert!(joined_h1_text);

            let mut byte_items = std::ptr::null_mut();
            let mut byte_elements = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_with_positions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut byte_items,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_extract_structure_elements_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut byte_elements,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_text_items_result_get_count(byte_items),
                item_count
            );
            assert_eq!(
                pdf_inspector_structure_elements_result_get_count(byte_elements),
                element_count
            );
            pdf_inspector_text_items_result_free(byte_items);
            pdf_inspector_structure_elements_result_free(byte_elements);

            let pages = [1];
            let mut filtered_items = std::ptr::null_mut();
            let mut filtered_elements = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_with_positions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    pages.as_ptr(),
                    pages.len(),
                    std::ptr::null(),
                    &mut filtered_items,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_extract_structure_elements_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    pages.as_ptr(),
                    pages.len(),
                    std::ptr::null(),
                    &mut filtered_elements,
                ),
                PdfInspectorError::Success
            );
            assert!(pdf_inspector_text_items_result_get_count(filtered_items) > 0);
            assert!(pdf_inspector_structure_elements_result_get_count(filtered_elements) > 0);
            assert!(
                (0..pdf_inspector_text_items_result_get_count(filtered_items)).all(|index| {
                    pdf_inspector_text_items_result_get_page(filtered_items, index) == 1
                })
            );
            assert!(
                (0..pdf_inspector_structure_elements_result_get_count(filtered_elements)).all(
                    |index| {
                        pdf_inspector_structure_elements_result_get_page(filtered_elements, index)
                            == 1
                    }
                )
            );
            pdf_inspector_text_items_result_free(filtered_items);
            pdf_inspector_structure_elements_result_free(filtered_elements);

            let mut invalid_items = std::ptr::NonNull::dangling().as_ptr();
            let mut invalid_elements = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_text_with_positions(
                    path.as_ptr(),
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut invalid_items,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(
                pdf_inspector_extract_structure_elements(
                    path.as_ptr(),
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut invalid_elements,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(invalid_items.is_null());
            assert!(invalid_elements.is_null());

            pdf_inspector_text_items_result_free(path_items);
            pdf_inspector_structure_elements_result_free(path_elements);
        }
    }

    #[test]
    fn ffi_input_validation() {
        unsafe {
            let mut pages_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_pages_markdown_mem(
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut pages_result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(
                pdf_inspector_process_pdf_mem(
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut std::ptr::null_mut(),
                ),
                PdfInspectorError::NullPointer
            );
            assert_ne!(
                pdf_inspector_process_pdf_mem(
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut std::ptr::null_mut(),
                ),
                PdfInspectorError::NullPointer
            );

            let options = pdf_inspector_options_new();
            for (threshold, expected) in [
                (f32::NAN, PdfInspectorError::InvalidArgument),
                (f32::INFINITY, PdfInspectorError::InvalidArgument),
                (-0.1, PdfInspectorError::InvalidArgument),
                (1.1, PdfInspectorError::InvalidArgument),
                (0.0, PdfInspectorError::Success),
                (1.0, PdfInspectorError::Success),
            ] {
                assert_eq!(
                    pdf_inspector_options_set_text_page_ratio_threshold(options, threshold),
                    expected,
                    "threshold {threshold}"
                );
            }
            pdf_inspector_options_free(options);
        }
    }

    /// Extracted text may contain NUL bytes (a `<0000>` ToUnicode destination is
    /// a routine producer artifact for unmapped glyphs). The borrowed view must
    /// carry them through instead of failing the whole document.
    #[test]
    fn extracted_text_with_interior_nul_round_trips() {
        unsafe {
            let text_result = Box::into_raw(Box::new(CTextResult {
                text: "A\0C\n".into(),
            }));
            let text =
                get_byte_view(|out| pdf_inspector_text_result_get_text(text_result, out)).unwrap();
            assert_eq!(text.len, 4);
            assert_eq!(std::slice::from_raw_parts(text.ptr, text.len), b"A\0C\n");
            pdf_inspector_text_result_free(text_result);

            assert!(get_byte_view(|out| {
                pdf_inspector_text_result_get_text(std::ptr::null(), out)
            })
            .is_none());
        }
    }

    /// Every page number crossing the C boundary is 1-indexed, including
    /// `extract_pages_markdown`, whose Rust counterpart is 0-indexed.
    #[test]
    fn pages_markdown_uses_one_indexed_pages() {
        unsafe {
            let buffer = std::fs::read(FIXTURE).unwrap();
            let mut result = std::ptr::null_mut();

            // Page 0 has no 1-indexed meaning.
            let zero = [0u32];
            assert_eq!(
                pdf_inspector_extract_pages_markdown_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    zero.as_ptr(),
                    zero.len(),
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(result.is_null());

            let first = [1u32];
            assert_eq!(
                pdf_inspector_extract_pages_markdown_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    first.as_ptr(),
                    first.len(),
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::Success
            );
            assert_eq!(pdf_inspector_pages_result_get_page_count(result), 1);
            // Reported back in the same base the caller asked in.
            assert_eq!(pdf_inspector_pages_result_get_page_number(result, 0), 1);
            pdf_inspector_pages_result_free(result);
        }
    }

    #[test]
    fn page_filter_can_be_cleared() {
        unsafe {
            let options = pdf_inspector_options_new();
            assert_eq!(
                pdf_inspector_options_add_page(options, 1),
                PdfInspectorError::Success
            );
            assert!((*options).page_filter.is_some());
            assert_eq!(
                pdf_inspector_options_clear_pages(options),
                PdfInspectorError::Success
            );
            assert!((*options).page_filter.is_none());
            assert_eq!(
                pdf_inspector_options_clear_pages(std::ptr::null_mut()),
                PdfInspectorError::NullPointer
            );
            pdf_inspector_options_free(options);
        }
    }

    /// Every mutating entry point rejects a NULL handle rather than panicking.
    #[test]
    fn null_options_handle_is_rejected() {
        unsafe {
            for (name, set) in BOOL_SETTERS {
                assert_eq!(
                    set(std::ptr::null_mut(), true),
                    PdfInspectorError::NullPointer,
                    "{name}"
                );
            }
            assert_eq!(
                pdf_inspector_options_set_mode(std::ptr::null_mut(), 0),
                PdfInspectorError::NullPointer
            );
            assert_eq!(
                pdf_inspector_options_set_profile(std::ptr::null_mut(), 0),
                PdfInspectorError::NullPointer
            );
            assert_eq!(
                pdf_inspector_options_set_password(std::ptr::null_mut(), std::ptr::null()),
                PdfInspectorError::NullPointer
            );
            assert_eq!(
                pdf_inspector_options_add_page(std::ptr::null_mut(), 1),
                PdfInspectorError::NullPointer
            );
            assert_eq!(
                pdf_inspector_options_set_min_text_ops_per_page(std::ptr::null_mut(), 1),
                PdfInspectorError::NullPointer
            );
            assert_eq!(
                pdf_inspector_options_set_text_page_ratio_threshold(std::ptr::null_mut(), 0.5),
                PdfInspectorError::NullPointer
            );
        }
    }

    /// Regression: a NULL `path` on these 5 entry points used to return
    /// before `*result_out` was zeroed, leaving a caller's stale pointer in
    /// place. `_mem` siblings were already correct; re-asserted here too.
    #[test]
    fn out_parameter_is_zeroed_before_any_other_validation_can_fail() {
        unsafe {
            // A non-NULL sentinel standing in for a caller-forgotten stale handle.
            let mut result: *mut CPdfProcessResult = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_process_pdf(std::ptr::null(), std::ptr::null(), &mut result),
                PdfInspectorError::NullPointer
            );
            assert!(result.is_null());

            let mut mem_result: *mut CPdfProcessResult = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_process_pdf_mem(
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    &mut mem_result,
                ),
                PdfInspectorError::NullPointer
            );
            assert!(mem_result.is_null());

            let mut text_result: *mut CTextResult = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_text(std::ptr::null(), std::ptr::null(), &mut text_result),
                PdfInspectorError::NullPointer
            );
            assert!(text_result.is_null());

            let mut items: *mut CTextItemsResult = std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_text_with_positions(
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut items,
                ),
                PdfInspectorError::NullPointer
            );
            assert!(items.is_null());

            let mut elements: *mut CStructureElementsResult =
                std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_structure_elements(
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut elements,
                ),
                PdfInspectorError::NullPointer
            );
            assert!(elements.is_null());

            let mut pages_result: *mut CPagesExtractionResult =
                std::ptr::NonNull::dangling().as_ptr();
            assert_eq!(
                pdf_inspector_extract_pages_markdown(
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    &mut pages_result,
                ),
                PdfInspectorError::NullPointer
            );
            assert!(pages_result.is_null());
        }
    }

    /// Page 0 must be rejected the same way at every page-list entry point.
    #[test]
    fn page_zero_is_rejected_at_every_page_list_entry_point() {
        unsafe {
            let options = pdf_inspector_options_new();
            assert_eq!(
                pdf_inspector_options_add_page(options, 0),
                PdfInspectorError::InvalidArgument
            );
            assert!((*options).page_filter.is_none());

            let zero = [0u32];
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Pages as i32,
                    0,
                    zero.as_ptr(),
                    zero.len(),
                ),
                PdfInspectorError::InvalidArgument
            );
            pdf_inspector_options_free(options);

            let buffer = std::fs::read(TAGGED_FIXTURE).unwrap();

            let mut items = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_with_positions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    zero.as_ptr(),
                    zero.len(),
                    std::ptr::null(),
                    &mut items,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(items.is_null());

            let mut elements = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_structure_elements_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    zero.as_ptr(),
                    zero.len(),
                    std::ptr::null(),
                    &mut elements,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(elements.is_null());

            // `extract_pages_markdown_mem` is covered by
            // `pages_markdown_uses_one_indexed_pages`.

            // Per-item page fields follow the same rule: a caller-built
            // descriptor and a table-detection rect on page 0 are rejected,
            // and the failed add appends nothing.
            let mut built = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_text_items_result_new(&mut built),
                PdfInspectorError::Success
            );
            let zero_page_descriptor = CTextItemDescriptor {
                page: 0,
                text: CByteView::default(),
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                font: CByteView::default(),
                font_size: 12.0,
                item_type: CTextItemType::Text as i32,
                link_url: CByteView::default(),
                flags: 0,
                mcid: 0,
            };
            assert_eq!(
                pdf_inspector_text_items_result_add(built, &zero_page_descriptor, 1),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(pdf_inspector_text_items_result_get_count(built), 0);

            let zero_page_rect = CPdfRect {
                page: 0,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            };
            let mut markdown = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_to_markdown_from_items(
                    built,
                    &zero_page_rect,
                    1,
                    0,
                    std::ptr::null(),
                    &mut markdown,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert!(markdown.is_null());
            pdf_inspector_text_items_result_free(built);
        }
    }

    /// 1-indexed, unlike the Rust `PdfClassification::pages_needing_ocr` it wraps.
    #[test]
    fn classification_pages_needing_ocr_are_one_indexed() {
        unsafe {
            let buffer =
                std::fs::read("tests/fixtures/vector_outlined_text_with_caption.pdf").unwrap();
            let mut classification = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_classify_pdf_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    &mut classification,
                ),
                PdfInspectorError::Success
            );
            assert!(!classification.is_null());

            let view = get_u32_view(|out| {
                pdf_inspector_classification_get_pages_needing_ocr(classification, out)
            })
            .unwrap();
            assert!(view.len > 0);
            let pages = std::slice::from_raw_parts(view.ptr, view.len);
            assert!(pages.contains(&1), "expected 1-indexed page 1 in {pages:?}");

            pdf_inspector_classification_free(classification);
        }
    }

    #[test]
    fn abi_minor_version_is_reported() {
        assert_eq!(pdf_inspector_abi_minor(), PDF_INSPECTOR_ABI_MINOR);
    }

    #[test]
    fn last_error_message_reports_diagnostics_and_clears_on_next_call() {
        unsafe {
            // A successful call carries no message.
            let bytes = std::fs::read(FIXTURE).unwrap();
            let mut text_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_mem(
                    bytes.as_ptr(),
                    bytes.len(),
                    std::ptr::null(),
                    &mut text_result,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_text_result_free(text_result);
            assert!(get_byte_view(|out| pdf_inspector_last_error_message(out)).is_none());

            // `NotAPdf` carries a human-readable file-type hint.
            let garbage = b"not a pdf";
            let mut bad_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_mem(
                    garbage.as_ptr(),
                    garbage.len(),
                    std::ptr::null(),
                    &mut bad_result,
                ),
                PdfInspectorError::NotAPdf
            );
            assert!(bad_result.is_null());
            let message = get_byte_view(|out| pdf_inspector_last_error_message(out)).unwrap();
            assert!(!message.ptr.is_null());
            assert!(message.len > 0);
            assert!(
                std::str::from_utf8(std::slice::from_raw_parts(message.ptr, message.len)).is_ok()
            );

            // The next call clears it, even though this failure (a NULL
            // pointer) carries no diagnostic text of its own.
            let mut ignored = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_mem(std::ptr::null(), 1, std::ptr::null(), &mut ignored,),
                PdfInspectorError::NullPointer
            );
            assert!(get_byte_view(|out| pdf_inspector_last_error_message(out)).is_none());
        }
    }

    #[test]
    fn getters_and_free_do_not_clear_the_last_error_slot() {
        unsafe {
            let mut result = std::ptr::null_mut();
            let garbage = b"not a pdf";
            assert_eq!(
                pdf_inspector_process_pdf_mem(
                    garbage.as_ptr(),
                    garbage.len(),
                    std::ptr::null(),
                    &mut result,
                ),
                PdfInspectorError::NotAPdf
            );
            let message = get_byte_view(|out| pdf_inspector_last_error_message(out)).unwrap();

            let _ = pdf_inspector_result_get_page_count(std::ptr::null());
            let current = get_byte_view(|out| pdf_inspector_last_error_message(out)).unwrap();
            assert_eq!(current.len, message.len);
            assert_eq!(current.ptr, message.ptr);

            let _ = pdf_inspector_text_items_result_get_count(std::ptr::null());
            let current = get_byte_view(|out| pdf_inspector_last_error_message(out)).unwrap();
            assert_eq!(current.len, message.len);
            assert_eq!(current.ptr, message.ptr);

            pdf_inspector_result_free(std::ptr::null_mut());
            let current = get_byte_view(|out| pdf_inspector_last_error_message(out)).unwrap();
            assert_eq!(current.len, message.len);
            assert_eq!(current.ptr, message.ptr);

            // Only a `PdfInspectorError`-returning call clears it.
            let path = CString::new(FIXTURE).unwrap();
            let mut ok_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_process_pdf(path.as_ptr(), std::ptr::null(), &mut ok_result),
                PdfInspectorError::Success
            );
            assert!(get_byte_view(|out| pdf_inspector_last_error_message(out)).is_none());
            pdf_inspector_result_free(ok_result);
        }
    }

    #[test]
    fn base_font_size_setter_uses_non_positive_sentinel_to_clear() {
        unsafe {
            let options = pdf_inspector_options_new();

            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 12.0),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, Some(12.0));

            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 0.0),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, None);

            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 12.0),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, -1.0),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, None);

            // A denormal must not sneak past a bare `> 0.0` check.
            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 12.0),
                PdfInspectorError::Success
            );
            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 1e-45),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, None);
            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 0.5),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, None);

            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, 1.0),
                PdfInspectorError::Success
            );
            assert_eq!((*options).markdown.base_font_size, Some(1.0));

            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, f32::NAN),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(
                pdf_inspector_options_set_base_font_size(options, f32::INFINITY),
                PdfInspectorError::InvalidArgument
            );

            pdf_inspector_options_free(options);
        }
    }

    #[test]
    fn scan_strategy_setter_validates_each_variant() {
        unsafe {
            let options = pdf_inspector_options_new();

            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::EarlyExit as i32,
                    0,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::Success
            );
            assert!(matches!(
                (*options).detection.strategy,
                crate::ScanStrategy::EarlyExit
            ));

            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Full as i32,
                    0,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::Success
            );
            assert!(matches!(
                (*options).detection.strategy,
                crate::ScanStrategy::Full
            ));

            // `Sample` needs a nonzero size.
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Sample as i32,
                    0,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::InvalidArgument
            );
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Sample as i32,
                    4,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::Success
            );
            assert!(matches!(
                (*options).detection.strategy,
                crate::ScanStrategy::Sample(4)
            ));

            // `Pages` needs a nonempty, zero-free page list.
            let pages = [1u32, 3];
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Pages as i32,
                    0,
                    pages.as_ptr(),
                    pages.len(),
                ),
                PdfInspectorError::Success
            );
            match &(*options).detection.strategy {
                crate::ScanStrategy::Pages(got) => assert_eq!(got.as_slice(), [1, 3]),
                other => panic!("expected ScanStrategy::Pages, got {other:?}"),
            }
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Pages as i32,
                    0,
                    std::ptr::null(),
                    0,
                ),
                PdfInspectorError::InvalidArgument
            );
            let zero = [0u32];
            assert_eq!(
                pdf_inspector_options_set_scan_strategy(
                    options,
                    CScanStrategy::Pages as i32,
                    0,
                    zero.as_ptr(),
                    zero.len(),
                ),
                PdfInspectorError::InvalidArgument
            );

            assert_eq!(
                pdf_inspector_options_set_scan_strategy(options, 99, 0, std::ptr::null(), 0),
                PdfInspectorError::InvalidArgument
            );

            pdf_inspector_options_free(options);
        }
    }

    /// The wrong password behaves identically to no password.
    #[test]
    fn password_protected_pdf_is_usable_across_the_c_abi() {
        unsafe {
            const ENCRYPTED_FIXTURE: &str = "tests/fixtures/encrypted-secret123.pdf";
            let path = CString::new(ENCRYPTED_FIXTURE).unwrap();
            let buffer = std::fs::read(ENCRYPTED_FIXTURE).unwrap();
            let password = CString::new("secret123").unwrap();
            let wrong_password = CString::new("not-the-password").unwrap();

            let mut text_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text(path.as_ptr(), std::ptr::null(), &mut text_result),
                PdfInspectorError::Encrypted
            );
            assert!(text_result.is_null());
            assert_eq!(
                pdf_inspector_extract_text_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    wrong_password.as_ptr(),
                    &mut text_result,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(text_result.is_null());

            assert_eq!(
                pdf_inspector_extract_text(path.as_ptr(), password.as_ptr(), &mut text_result),
                PdfInspectorError::Success
            );
            assert!(
                get_byte_view(|out| pdf_inspector_text_result_get_text(text_result, out))
                    .unwrap()
                    .len
                    > 0
            );
            pdf_inspector_text_result_free(text_result);

            let mut text_result_mem = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    password.as_ptr(),
                    &mut text_result_mem,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_text_result_free(text_result_mem);

            let mut items = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_with_positions(
                    path.as_ptr(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut items,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_text_items_result_free(items);

            let mut items_mem = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_with_positions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut items_mem,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_text_items_result_free(items_mem);

            let mut elements = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_structure_elements(
                    path.as_ptr(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut elements,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_structure_elements_result_free(elements);

            let mut elements_mem = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_structure_elements_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut elements_mem,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_structure_elements_result_free(elements_mem);

            let mut pages_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_pages_markdown(
                    path.as_ptr(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut pages_result,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_pages_result_free(pages_result);

            let mut pages_result_mem = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_pages_markdown_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    0,
                    password.as_ptr(),
                    &mut pages_result_mem,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_pages_result_free(pages_result_mem);

            let mut classification = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_classify_pdf_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    std::ptr::null(),
                    &mut classification,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(classification.is_null());
            assert_eq!(
                pdf_inspector_classify_pdf_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    password.as_ptr(),
                    &mut classification,
                ),
                PdfInspectorError::Success
            );
            pdf_inspector_classification_free(classification);

            let regions = [CRegion {
                x1: 0.0,
                y1: 0.0,
                x2: 2_000.0,
                y2: 2_000.0,
            }];
            let page_regions = [CPageRegions {
                page: 1,
                regions: regions.as_ptr(),
                regions_count: regions.len(),
            }];
            let mut region_result = std::ptr::null_mut();
            assert_eq!(
                pdf_inspector_extract_text_in_regions_mem(
                    buffer.as_ptr(),
                    buffer.len(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    wrong_password.as_ptr(),
                    &mut region_result,
                ),
                PdfInspectorError::Encrypted
            );
            assert!(region_result.is_null());
            assert_eq!(
                pdf_inspector_extract_text_in_regions(
                    path.as_ptr(),
                    page_regions.as_ptr(),
                    page_regions.len(),
                    password.as_ptr(),
                    &mut region_result,
                ),
                PdfInspectorError::Success
            );
            assert!(
                get_byte_view(|out| {
                    pdf_inspector_region_text_result_get_text(region_result, 0, 0, out)
                })
                .unwrap()
                .len > 0
            );
            pdf_inspector_region_text_result_free(region_result);
        }
    }
}
