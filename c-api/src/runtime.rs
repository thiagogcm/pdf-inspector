use super::*;

pub(super) fn defaults() -> PdfRuntimeOptions {
    PdfRuntimeOptions {
        capabilities: PDF_CAP_RENDER | PDF_CAP_OCR,
        download_policy: PDF_DOWNLOAD_OFFLINE,
        ..PdfRuntimeOptions::default()
    }
}

/// Process-wide PDFium handle, loaded on first success. A failed load is
/// reported and retried on the next call.
#[cfg(feature = "render-pdfium")]
pub(super) fn renderer() -> Fallible<&'static pdf_inspector::vision::PdfiumRenderer> {
    use std::sync::OnceLock;
    static RENDERER: OnceLock<pdf_inspector::vision::PdfiumRenderer> = OnceLock::new();
    if let Some(renderer) = RENDERER.get() {
        return Ok(renderer);
    }
    let loaded = pdf_inspector::vision::PdfiumRenderer::load().map_err(Failure::runtime)?;
    Ok(RENDERER.get_or_init(|| loaded))
}

pub(super) unsafe fn prepare(options: &PdfRuntimeOptions) -> Fallible<PdfResult> {
    if options.capabilities == 0 || options.capabilities & !(PDF_CAP_RENDER | PDF_CAP_OCR) != 0 {
        return Err(Failure::invalid(
            "request rendering and/or OCR capabilities",
        ));
    }
    if options.download_policy > PDF_DOWNLOAD_OFFLINE {
        return Err(Failure::invalid("unknown model download policy"));
    }
    let model_directory = input::optional_text(options.model_directory)?;
    if model_directory.is_some() {
        input::path(options.model_directory)?;
    }
    let requested = options.capabilities
        | if options.capabilities & PDF_CAP_OCR != 0 {
            PDF_CAP_RENDER
        } else {
            0
        };
    if requested & !pdf_inspector_capabilities() != 0 {
        return Err(Failure::unsupported(
            "requested native runtime was not compiled in",
        ));
    }
    let mut storage = Storage::default();
    let mut info = PdfRuntimeInfo {
        ready: requested,
        ..PdfRuntimeInfo::default()
    };
    // Check PDFium before any model acquisition, just as the processing path does.
    #[cfg(feature = "render-pdfium")]
    renderer()?;
    #[cfg(feature = "ocr")]
    if requested & PDF_CAP_OCR != 0 {
        use pdf_inspector::vision::{ModelDownloadPolicy, OcrOptions, PP_OCR_V6_SMALL};
        let options = OcrOptions {
            model_directory: model_directory.map(Into::into),
            model_downloads: if options.download_policy == PDF_DOWNLOAD_OFFLINE {
                ModelDownloadPolicy::Offline
            } else {
                ModelDownloadPolicy::IfMissing
            },
            ..OcrOptions::default()
        };
        pdf_inspector::vision::cached_ocr_engine(&options).map_err(Failure::runtime)?;
        info.model = storage.bytes(PP_OCR_V6_SMALL.id.as_bytes());
        info.model_revision = storage.bytes(PP_OCR_V6_SMALL.revision.as_bytes());
    }
    // Keep feature-minimal builds warning-free without changing the report layout.
    let _ = (&mut storage, &mut info);
    Ok(PdfResult {
        view: PdfResultView {
            present: PDF_RUNTIME,
            runtime: info,
            ..PdfResultView::default()
        },
        _storage: storage,
    })
}
