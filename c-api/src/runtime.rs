use super::*;

pub(super) fn defaults() -> PdfRuntimeOptions {
    PdfRuntimeOptions {
        capabilities: PDF_CAP_RENDER | PDF_CAP_OCR,
        download_policy: PDF_DOWNLOAD_OFFLINE,
        ..PdfRuntimeOptions::default()
    }
}

/// Process-wide PDFium handle: the file `load_pdfium` named, or else the one
/// the first use found through firecrawl-pdfium's search.
#[cfg(feature = "render-pdfium")]
static RENDERER: std::sync::OnceLock<pdf_inspector::vision::PdfiumRenderer> =
    std::sync::OnceLock::new();

/// Process-wide PDFium handle, loaded on first success. A failed load is
/// reported and retried on the next call.
#[cfg(feature = "render-pdfium")]
pub(super) fn renderer() -> Fallible<&'static pdf_inspector::vision::PdfiumRenderer> {
    if let Some(renderer) = RENDERER.get() {
        return Ok(renderer);
    }
    let loaded = pdf_inspector::vision::PdfiumRenderer::load().map_err(Failure::runtime)?;
    Ok(RENDERER.get_or_init(|| loaded))
}

/// A runtime library path the dynamic linker opens as given: absolute, so no
/// search can resolve it to another file.
fn absolute<'a>(path: &'a str, library: &str) -> Fallible<&'a std::path::Path> {
    let path = std::path::Path::new(path);
    if !path.is_absolute() {
        return Err(Failure::invalid(format!(
            "the {library} path must be absolute, so no search resolves it: {}",
            path.display()
        )));
    }
    Ok(path)
}

/// `error`'s causes, without its own text: the loaders' top-level messages
/// recommend environment variables a caller naming the file does not use.
#[cfg_attr(not(feature = "render-pdfium"), allow(dead_code))]
fn causes(error: &dyn std::error::Error) -> String {
    let mut parts = Vec::new();
    let mut cause = error.source();
    while let Some(current) = cause {
        parts.push(current.to_string());
        cause = current.source();
    }
    if parts.is_empty() {
        error.to_string()
    } else {
        parts.join(": ")
    }
}

/// Loads PDFium from `path` for the process, before any render or OCR call
/// would search for it.
pub(super) fn load_pdfium(path: &str) -> Fallible<()> {
    let path = absolute(path, "PDFium")?;
    #[cfg(feature = "render-pdfium")]
    {
        if let Some(loaded) = RENDERER.get() {
            let requested = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            return match loaded.loaded_from() {
                Some(existing) if existing == requested => Ok(()),
                existing => Err(Failure::invalid(format!(
                    "PDFium is already loaded from {}; a process holds one PDFium, so {} is not loaded",
                    existing.map_or_else(|| "the system search path".into(), |p| p.display().to_string()),
                    path.display()
                ))),
            };
        }
        let loaded =
            pdf_inspector::vision::PdfiumRenderer::load_from_path(path).map_err(|error| {
                Failure::runtime(format!(
                    "could not load PDFium from {}: {}",
                    path.display(),
                    causes(&error)
                ))
            })?;
        RENDERER.get_or_init(|| loaded);
        Ok(())
    }
    #[cfg(not(feature = "render-pdfium"))]
    {
        let _ = path;
        Err(Failure::unsupported("rendering was not compiled in"))
    }
}

/// The file `load_onnx_runtime` loaded; held under the lock so two racing
/// calls cannot load two libraries.
#[cfg(feature = "ocr")]
static ONNX_RUNTIME: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Loads ONNX Runtime from `path` for the process, before OCR would look it
/// up through `ORT_DYLIB_PATH` or the platform library name.
pub(super) fn load_onnx_runtime(path: &str) -> Fallible<()> {
    let path = absolute(path, "ONNX Runtime")?;
    #[cfg(feature = "ocr")]
    {
        let mut loaded = ONNX_RUNTIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match loaded.as_deref() {
            Some(existing) if existing == path => return Ok(()),
            Some(existing) => {
                return Err(Failure::invalid(format!(
                    "ONNX Runtime is already loaded from {}; a process holds one ONNX Runtime, so {} is not loaded",
                    existing.display(),
                    path.display()
                )));
            }
            None => {}
        }
        // `ort` keeps the first library it loads for the process; OCR's own
        // later lookup then finds it loaded and makes no search.
        drop(ort::init_from(path).map_err(|error| {
            Failure::runtime(format!(
                "could not load ONNX Runtime from {}: {error}",
                path.display()
            ))
        })?);
        *loaded = Some(path.to_path_buf());
        Ok(())
    }
    #[cfg(not(feature = "ocr"))]
    {
        let _ = path;
        Err(Failure::unsupported("native OCR was not compiled in"))
    }
}

pub(super) unsafe fn prepare(options: &PdfRuntimeOptions) -> Fallible<PdfRuntimeInfo> {
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
    #[cfg_attr(not(feature = "ocr"), allow(unused_mut))]
    let mut info = PdfRuntimeInfo {
        capabilities: requested,
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
        info.model = PdfBytes::from_static(PP_OCR_V6_SMALL.id);
        info.model_revision = PdfBytes::from_static(PP_OCR_V6_SMALL.revision);
    }
    Ok(info)
}
