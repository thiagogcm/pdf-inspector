//! Document-oriented C interface.
//!
//! # Safety
//! Every non-NULL input pointer must be aligned and reference live memory of
//! the declared type and length. Inputs must not be mutated during a call.
//! Output slots must be writable and must not alias inputs or each other.
//! Published pointers come from this library and are freed exactly once with
//! their matching function; free must not race with any use of that record
//! or the document info. NULL free is a no-op. Calls are synchronous.
//! Borrowed request memory is never retained.
#![allow(clippy::missing_safety_doc)]

mod content;
mod execute;
mod frames;
mod input;
mod layout;
mod links;
mod output;
mod runtime;
mod semantic;
mod tables;
#[cfg(test)]
mod tests;
mod types;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
pub use types::*;

use execute::DocumentState;
use output::Storage;

/// Owns decrypted document bytes and page geometry. Immutable after open, so
/// any number of executions may run against it concurrently.
pub struct PdfDocument {
    state: DocumentState,
}
/// Backs a published `PdfResult`: the record sits first so the pointer C
/// holds is also the allocation.
#[repr(C)]
struct ResultOwner {
    result: PdfResult,
    storage: Storage,
}
#[repr(C)]
struct ErrorOwner {
    error: PdfError,
    storage: Storage,
}

/// A boxed owner whose `Public` record is what C receives.
///
/// # Safety
/// `Public` is `Self`, or the field at offset 0 of `#[repr(C)] Self`, so the
/// pointer C holds is also the allocation.
unsafe trait Owner: Sized {
    type Public;
    fn publish(self) -> *mut Self::Public {
        Box::into_raw(Box::new(self)).cast()
    }
    unsafe fn release(public: *mut Self::Public) {
        drop(Box::from_raw(public.cast::<Self>()));
    }
}
/// Implement `Owner` for a record-first owner, checking the offset.
macro_rules! owner {
    ($owner:ident . $field:ident : $public:ty) => {
        const _: () = assert!(std::mem::offset_of!($owner, $field) == 0);
        unsafe impl Owner for $owner {
            type Public = $public;
        }
    };
}
owner!(ResultOwner.result: PdfResult);
owner!(ErrorOwner.error: PdfError);
unsafe impl Owner for PdfDocument {
    type Public = PdfDocument;
}
impl ResultOwner {
    fn new(result: PdfResult, storage: Storage) -> Self {
        Self { result, storage }
    }
}
impl ErrorOwner {
    fn new(failure: Failure) -> Self {
        let storage = Storage::default();
        let error = PdfError {
            status: failure.status,
            message: storage.bytes(&failure.message),
        };
        Self { error, storage }
    }
}

#[derive(Debug)]
struct Failure {
    status: i32,
    message: String,
}
type Fallible<T> = Result<T, Failure>;
impl Failure {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            status: PDF_INVALID_ARGUMENT,
            message: message.into(),
        }
    }
    fn unsupported(message: &str) -> Self {
        Self {
            status: PDF_UNSUPPORTED,
            message: message.into(),
        }
    }
    fn parse(message: impl Into<String>) -> Self {
        Self {
            status: PDF_PARSE_ERROR,
            message: message.into(),
        }
    }
    fn runtime(error: impl std::fmt::Display) -> Self {
        Self {
            status: PDF_RUNTIME_ERROR,
            message: error.to_string(),
        }
    }
}
impl From<pdf_inspector::PdfError> for Failure {
    fn from(e: pdf_inspector::PdfError) -> Self {
        let status = match &e {
            pdf_inspector::PdfError::Io(_) => PDF_IO_ERROR,
            pdf_inspector::PdfError::Encrypted => PDF_PASSWORD_ERROR,
            _ => PDF_PARSE_ERROR,
        };
        Self {
            status,
            message: e.to_string(),
        }
    }
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self {
            status: PDF_IO_ERROR,
            message: e.to_string(),
        }
    }
}

/// Publish a failure to `error` (when non-NULL) and return its status.
unsafe fn fail(error: *mut *mut PdfError, failure: Failure) -> i32 {
    let status = failure.status;
    if !error.is_null() {
        error.write(ErrorOwner::new(failure).publish());
    }
    status
}
/// Write `f`'s value to `out`, which is cleared to `empty` before validation.
/// Panics are caught and reported as `PDF_PANIC`.
unsafe fn publish_value<T>(
    out: *mut T,
    empty: T,
    error: *mut *mut PdfError,
    f: impl FnOnce() -> Fallible<T>,
) -> i32 {
    if !error.is_null() {
        error.write(ptr::null_mut());
    }
    let Some(slot) = out.as_mut() else {
        return fail(error, Failure::invalid("output slot is NULL"));
    };
    *slot = empty;
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => {
            *slot = value;
            PDF_OK
        }
        Ok(Err(failure)) => fail(error, failure),
        Err(_) => fail(
            error,
            Failure {
                status: PDF_PANIC,
                message: "processing panicked".into(),
            },
        ),
    }
}
/// Publish an owner's pointer to `out`.
unsafe fn publish<O: Owner>(
    out: *mut *mut O::Public,
    error: *mut *mut PdfError,
    f: impl FnOnce() -> Fallible<O>,
) -> i32 {
    publish_value(out, ptr::null_mut(), error, || f().map(Owner::publish))
}
unsafe fn release<O: Owner>(public: *mut O::Public) {
    if !public.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| O::release(public)));
    }
}
unsafe fn init<T>(out: *mut T, value: T) -> i32 {
    if out.is_null() {
        return PDF_INVALID_ARGUMENT;
    }
    out.write(value);
    PDF_OK
}
unsafe fn markdown_options(
    options: *const PdfMarkdownOptions,
) -> Fallible<pdf_inspector::MarkdownOptions> {
    input::markdown(&input::or_default(
        options,
        "Markdown options",
        input::default_markdown,
    )?)
}

/// Library version as static UTF-8.
#[no_mangle]
pub extern "C" fn pdf_inspector_version() -> PdfBytes {
    PdfBytes::from_static(env!("CARGO_PKG_VERSION"))
}
/// Compiled capabilities. Does not load libraries, inspect models, or use the network.
#[no_mangle]
pub extern "C" fn pdf_inspector_capabilities() -> u32 {
    PDF_CAP_EXTERNAL_OCR
        | if cfg!(feature = "render-pdfium") {
            PDF_CAP_RENDER
        } else {
            0
        }
        | if cfg!(feature = "ocr") {
            PDF_CAP_OCR
        } else {
            0
        }
        | if cfg!(feature = "model-download") {
            PDF_CAP_DOWNLOAD
        } else {
            0
        }
}
/// Initialize options. Returns PDF_INVALID_ARGUMENT for NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_runtime_options_init(out: *mut PdfRuntimeOptions) -> i32 {
    init(out, runtime::defaults())
}
/// Verify native runtime readiness without a document, initializing reusable
/// OCR sessions. NULL options request rendering and OCR, offline. `out` is
/// zeroed first and written only on success.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_prepare_runtime(
    options: *const PdfRuntimeOptions,
    out: *mut PdfRuntimeInfo,
    error: *mut *mut PdfError,
) -> i32 {
    publish_value(out, PdfRuntimeInfo::default(), error, || {
        runtime::prepare(&input::or_default(
            options,
            "runtime options",
            runtime::defaults,
        )?)
    })
}
/// Initialize a request to all pages, inspection and Markdown, with OCR off.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_request_init(out: *mut PdfRequest) -> i32 {
    init(out, input::default_request())
}
/// Initialize Markdown options to the core defaults.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_markdown_options_init(out: *mut PdfMarkdownOptions) -> i32 {
    init(out, input::default_markdown())
}
/// Open bytes or a UTF-8 path. Source and password memory may be released on return.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_open(
    source: *const PdfSource,
    out: *mut *mut PdfDocument,
    error: *mut *mut PdfError,
) -> i32 {
    publish(out, error, || {
        let source = input::required(source, "source")?;
        let bytes = match source.kind {
            PDF_SOURCE_BYTES => input::bytes(source.data)?.to_vec(),
            PDF_SOURCE_PATH => std::fs::read(input::path(source.data)?)?,
            _ => return Err(Failure::invalid("invalid source kind")),
        };
        let password = input::optional_text(source.password)?;
        Ok(PdfDocument {
            state: DocumentState::open(bytes, password)?,
        })
    })
}
/// Borrow page count, sheet-frame page dimensions, and the load audit; NULL returns NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_document_info(
    document: *const PdfDocument,
) -> *const PdfDocumentInfo {
    document.as_ref().map_or(ptr::null(), |d| &d.state.info)
}
/// Execute against an open document. NULL request uses initialized defaults.
/// Each call publishes an independent result or an independent error.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_execute(
    document: *const PdfDocument,
    request: *const PdfRequest,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfError,
) -> i32 {
    publish(out, error, || {
        let document = input::required(document, "document")?;
        let request = input::or_default(request, "request", input::default_request)?;
        execute::run(&document.state, &request)
    })
}
/// Compose Markdown from plain UTF-8 text. NULL options use the core defaults.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_compose_text(
    text: PdfBytes,
    options: *const PdfMarkdownOptions,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfError,
) -> i32 {
    publish(out, error, || {
        let options = markdown_options(options)?;
        execute::compose_text(text, options)
    })
}
/// Compose Markdown from positioned items, without a document handle. NULL
/// options use the core defaults.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_compose_items(
    input: *const PdfComposeInput,
    options: *const PdfMarkdownOptions,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfError,
) -> i32 {
    publish(out, error, || {
        let options = markdown_options(options)?;
        execute::compose_items(input::required(input, "composition input")?, options)
    })
}
/// Release a document and its info. Existing results remain valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_document_free(document: *mut PdfDocument) {
    release::<PdfDocument>(document);
}
/// Release a result and everything reachable from it.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_free(result: *mut PdfResult) {
    release::<ResultOwner>(result);
}
/// Release an error and its message.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_error_free(error: *mut PdfError) {
    release::<ErrorOwner>(error);
}
