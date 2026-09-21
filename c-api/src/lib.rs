//! Document-oriented C interface.
//!
//! # Safety
//! Every non-NULL input pointer must be aligned and reference live memory of
//! the declared type and length. Inputs must not be mutated during a call.
//! Output slots must be writable and must not alias inputs or each other.
//! Handles must come from this library and be freed exactly once. Free must
//! not race with any use of that handle or its views. NULL free is a no-op.
//! Calls are synchronous. Borrowed request memory is never retained.
#![allow(clippy::missing_safety_doc)]

mod execute;
mod input;
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
/// Owns a result and all memory reachable through its view.
pub struct PdfResult {
    view: PdfResultView,
    _storage: Storage,
}
/// Owns the diagnostic for a single failed call.
pub struct PdfErrorHandle {
    view: PdfDiagnostic,
    _message: Box<[u8]>,
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
    #[cfg(feature = "render-pdfium")]
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

unsafe fn publish<T>(
    out: *mut *mut T,
    error: *mut *mut PdfErrorHandle,
    f: impl FnOnce() -> Fallible<T>,
) -> i32 {
    if !error.is_null() {
        error.write(ptr::null_mut());
    }
    if !out.is_null() {
        out.write(ptr::null_mut());
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        if out.is_null() {
            return Err(Failure::invalid("output slot is NULL"));
        }
        f()
    }));
    match result {
        Ok(Ok(value)) => {
            out.write(Box::into_raw(Box::new(value)));
            PDF_OK
        }
        failure => {
            let failure = match failure {
                Ok(Err(e)) => e,
                _ => Failure {
                    status: PDF_PANIC,
                    message: "processing panicked".into(),
                },
            };
            if !error.is_null() {
                let bytes = failure.message.into_bytes().into_boxed_slice();
                let view = PdfDiagnostic {
                    status: failure.status,
                    message: PdfBytes {
                        ptr: bytes.as_ptr(),
                        len: bytes.len(),
                    },
                };
                error.write(Box::into_raw(Box::new(PdfErrorHandle {
                    view,
                    _message: bytes,
                })));
            }
            failure.status
        }
    }
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
    if out.is_null() {
        return PDF_INVALID_ARGUMENT;
    }
    out.write(runtime::defaults());
    PDF_OK
}
/// Verify native runtime readiness without a document, initializing reusable OCR sessions.
/// NULL options request rendering and OCR, offline. Downloads require explicit opt-in.
/// Returns a runtime result or an owned diagnostic; use the ordinary result/error functions.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_prepare_runtime(
    options: *const PdfRuntimeOptions,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfErrorHandle,
) -> i32 {
    publish(out, error, || {
        let options = if options.is_null() {
            runtime::defaults()
        } else {
            *input::required(options, "runtime options")?
        };
        runtime::prepare(&options)
    })
}
/// Initialize options. Returns PDF_INVALID_ARGUMENT for NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_open_options_init(out: *mut PdfOpenOptions) -> i32 {
    if out.is_null() {
        return PDF_INVALID_ARGUMENT;
    }
    out.write(PdfOpenOptions::default());
    PDF_OK
}
/// Initialize a request to all pages, inspection and Markdown, with OCR off.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_request_init(out: *mut PdfRequest) -> i32 {
    if out.is_null() {
        return PDF_INVALID_ARGUMENT;
    }
    out.write(input::default_request());
    PDF_OK
}
/// Initialize composition options for plain UTF-8 text.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_compose_init(out: *mut PdfComposeInput) -> i32 {
    if out.is_null() {
        return PDF_INVALID_ARGUMENT;
    }
    out.write(PdfComposeInput {
        markdown: input::default_markdown(),
        ..PdfComposeInput::default()
    });
    PDF_OK
}
/// Open bytes or a UTF-8 path. Source and password memory may be released on return.
/// NULL options use the default empty-password behavior.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_open(
    source: *const PdfSource,
    options: *const PdfOpenOptions,
    out: *mut *mut PdfDocument,
    error: *mut *mut PdfErrorHandle,
) -> i32 {
    publish(out, error, || {
        let source = input::required(source, "source")?;
        let bytes = match source.kind {
            PDF_SOURCE_BYTES => input::bytes(source.data)?.to_vec(),
            PDF_SOURCE_PATH => std::fs::read(input::path(source.data)?)?,
            _ => return Err(Failure::invalid("invalid source kind")),
        };
        let password = if options.is_null() {
            None
        } else {
            input::optional_text((*options).password)?.map(str::to_owned)
        };
        Ok(PdfDocument {
            state: DocumentState::open(bytes, password.as_deref())?,
        })
    })
}
/// Execute against an open document. NULL request uses initialized defaults.
/// Each call publishes an independent result or an independent error.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_execute(
    document: *const PdfDocument,
    request: *const PdfRequest,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfErrorHandle,
) -> i32 {
    publish(out, error, || {
        let document = input::required(document, "document")?;
        let request = if request.is_null() {
            input::default_request()
        } else {
            *request
        };
        execute::run(&document.state, &request)
    })
}
/// Compose Markdown from plain text or positioned items, without a document handle.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_compose(
    input: *const PdfComposeInput,
    out: *mut *mut PdfResult,
    error: *mut *mut PdfErrorHandle,
) -> i32 {
    publish(out, error, || {
        execute::compose(input::required(input, "composition input")?)
    })
}
/// Borrow the immutable root view; NULL returns NULL.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_view(
    result: *const PdfResult,
) -> *const PdfResultView {
    result.as_ref().map_or(ptr::null(), |r| &r.view)
}
/// Borrow a diagnostic; NULL returns NULL. Other calls cannot invalidate it.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_error_view(
    error: *const PdfErrorHandle,
) -> *const PdfDiagnostic {
    error.as_ref().map_or(ptr::null(), |e| &e.view)
}
/// Release a document. Existing results remain valid.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_document_free(document: *mut PdfDocument) {
    if !document.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(document))));
    }
}
/// Release a result and invalidate all views obtained from it.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_result_free(result: *mut PdfResult) {
    if !result.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(result))));
    }
}
/// Release an error and invalidate its diagnostic view.
#[no_mangle]
pub unsafe extern "C" fn pdf_inspector_error_free(error: *mut PdfErrorHandle) {
    if !error.is_null() {
        drop(Box::from_raw(error));
    }
}
