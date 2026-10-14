# C API observations

The exported records in `pdf_inspector.h` are owned by the document or execution result. Copy observations before freeing their owner. The header and handwritten consumers evolve together.

`PdfPageInfo.user_unit` is the page-local `/UserUnit`, defaulting to 1. Opening refuses nonnumeric values and values outside `(0, 75000]`. Page sizes, positioned geometry and OCR polygons remain in native user-space units. Convert from sheet to display space first, then multiply by this value for physical points. The pinned PDFium reports native units for both page dimensions and pixel-to-page transforms.

`PDF_OUT_OCR_SPANS` runs the core's public renderer/OCR orchestration and returns raw recognition independently from Markdown fusion. It cannot be combined with `PDF_OUT_MARKDOWN`; `PDF_OUT_TEXT` still requests native text. OCR routing and minimum confidence come from the request. Spans keep exact text, polygons and individual confidence in the request frame. Page provenance records model identity and timing. The C API retains observations while releasing each page's bitmap before recognizing the next page.

An `ANALYSIS` request returns `PdfPage.image_coverage` when `PDF_PAGE_IMAGE_COVERAGE` is set. This is the detector's existing 64-by-64 grid estimate of executed image coverage, bounded by the sum of painted image areas. It follows its clipping, form and pattern rules; resource inventory alone is not coverage. An incomplete walk, failed page-stream decoding, or a filtered page stream exceeding a default 32 MiB decoded validation budget leaves coverage absent. Image DPI remains unavailable.

Run `cargo test --manifest-path c-api/Cargo.toml`, `cargo clippy --manifest-path c-api/Cargo.toml --all-targets --features render-pdfium,ocr -- -D warnings` and `c-api/scripts/test-c-consumer.sh` for the C API checks. Rendering and recognition require explicitly configured PDFium, ONNX Runtime and models.

For the runtime smoke tests, set `PDFIUM_LIB_PATH` and `ORT_DYLIB_PATH` to absolute library files and `PDF_INSPECTOR_OCR_TEST_MODELS` to the provisioned model directory. When models are supplied, both library paths are required. The test loads the libraries through the C API before preparing OCR sessions, matching a host's initialization order. Run the render/OCR feature suite with these values to exercise actual recognition and both fused Markdown and raw-span provenance.
