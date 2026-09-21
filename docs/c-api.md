# C API

The C interface opens a document once and processes requests against it. A request selects outputs and can include region queries, table structure, or external OCR. Results expose typed arrays and UTF-8 views under one owner.

## Build

```sh
cargo build --manifest-path c-api/Cargo.toml --release
```

Artifacts are written to `c-api/target/release/`. The C crate depends on the root Rust library and forwards its optional rendering and OCR features. It has no Python bindings and requires no C feature in the core crate.

This produces a static library and an unversioned shared library: `libpdf_inspector_c.so` on Linux, `libpdf_inspector_c.dylib` on macOS, or `pdf_inspector_c.dll` on Windows. Build the generated `c-api/pdf_inspector.h` and library from the same checkout.

| Features | Available operations |
| --- | --- |
| Default | Inspection, native extraction, composition, regions, tables, external OCR fusion |
| `render-pdfium` | Also render pages using an externally installed PDFium |
| `ocr` | Also run selective native OCR with external models and ONNX Runtime |

The exported functions and record layouts are the same for all three builds. `pdf_inspector_capabilities()` reports compiled capabilities without loading dependencies. Requesting unavailable native processing returns `PDF_UNSUPPORTED`. A compiled capability does not imply that its runtime libraries or models are installed.

Run `c-api/scripts/generate-c-header.sh` after changing public C records or functions. It requires the cbindgen release pinned in that script. `c-api/scripts/test-c-consumer.sh` checks C/Rust record layouts, compiles the C consumer, and exercises linking and execution.

## Crate boundary

The C adapter lives in `c-api/`, following the independent-crate layout of `napi/` and `wasm/`. It owns ABI records, pointers, handles, validation, result storage, exported symbols, the generated header, linker settings, and consumer tests. Its library is named `pdf_inspector_c` so its artifacts cannot overwrite the core/Python library. There is no root `c-api` feature or compatibility forwarding layer.

The adapter runs the core's byte-oriented public API, exactly as the Node and Python bindings do. Each operation parses the retained bytes again; that is by design, and it keeps the core crate at upstream. The remaining core differences are small and additive:

| Core change | Kind | Why it remains |
| --- | --- | --- |
| `validate_pdf_bytes`, `load_document_from_mem_with_password` | visibility | Open validates and decrypts once with the core loader and its repair heuristics |
| `extractor::{visible_page_box, PageBox}` | visibility | Page dimensions and the coordinate frame every view is converted into |
| `extract_pages_markdown_mem_with_options` | additive wrapper | Per-page Markdown with a password and the request's Markdown options |
| `extractor::extract_positioned_page_content_mem` | additive wrapper | Items, rectangles, lines, thresholds, and rotations from one parse, with `PositionOptions` |
| `markdown::{MarkdownDocumentContext, to_markdown_from_items_with_rects_and_lines}` | visibility | Role-aware composition without a document |
| `markdown::detect_data_tables_from_items` | additive output mode | Logical data tables from the same detector that produces Markdown tables |
| `structure_tree::StructRole::from_name` | visibility | Caller-supplied structure roles for composition |
| `vision::cached_ocr_engine` | visibility | Runtime preparation warms the same process-cached OCR sessions |
| Complete-table helpers gated on `vision` instead of `ocr` | cfg fix | Upstream's `vision`-only build does not compile; the C crate builds the core with `vision` |

Run core checks from the repository root and binding checks explicitly:

```sh
cargo fmt --manifest-path c-api/Cargo.toml -- --check
cargo clippy --manifest-path c-api/Cargo.toml --all-features --all-targets -- -D warnings
cargo test --manifest-path c-api/Cargo.toml --all-features
c-api/scripts/test-c-consumer.sh
```

## Open, execute, and release

```c
#include "pdf_inspector.h"
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    PdfSource source = {
        PDF_SOURCE_PATH,
        {(const uint8_t *)argv[1], strlen(argv[1])}
    };
    PdfDocument *document = NULL;
    PdfResult *result = NULL;
    PdfErrorHandle *error = NULL;
    PdfRequest request;
    pdf_inspector_request_init(&request);
    request.outputs |= PDF_ITEMS | PDF_STRUCTURE;

    int32_t status = pdf_inspector_open(&source, NULL, &document, &error);
    if (status == PDF_OK)
        status = pdf_inspector_execute(document, &request, &result, &error);
    if (status == PDF_OK) {
        const PdfResultView *view = pdf_inspector_result_view(result);
        fwrite(view->markdown.ptr, 1, view->markdown.len, stdout);
        for (size_t p = 0; p < view->pages.len; ++p) {
            const PdfPage *page = &view->pages.ptr[p];
            for (size_t i = 0; i < page->items.len; ++i) {
                const PdfItem *item = &page->items.ptr[i];
                /* item contains geometry, style, font information, and text */
                (void)item;
            }
        }
    } else if (error) {
        const PdfDiagnostic *diagnostic = pdf_inspector_error_view(error);
        fwrite(diagnostic->message.ptr, 1, diagnostic->message.len, stderr);
    }
    pdf_inspector_error_free(error);
    pdf_inspector_result_free(result);
    pdf_inspector_document_free(document);
    return status == PDF_OK ? 0 : 1;
}
```

`PDF_SOURCE_BYTES` accepts a PDF byte slice. `PDF_SOURCE_PATH` accepts a length-delimited UTF-8 path without embedded NULs. Open copies or reads the source before returning; the caller can then release its input memory or remove the source file. `PdfOpenOptions.password` supplies a UTF-8 password; NULL options use the loader's empty-password behavior. A document that needed the password is decrypted once at open and retained in decrypted form, so every later operation works without it. Form XObjects whose `/BBox` has no area are widened the same way the core loader repairs them, so later rendering and OCR see the repaired bytes.

Initialize requests with `pdf_inspector_request_init`. NULL requests have the same defaults: all pages, inspection and document/page Markdown, upstream formatting and detection defaults, OCR off, the sheet coordinate frame, `bold_from_weight` off, and a bold weight threshold of 600. A page list is a set: numbers are 1-based, validated against the document, deduplicated, and returned in document order. An empty selection means all pages. The detector has its own optional page selection.

## Result projections

| Output flag | Projection |
| --- | --- |
| `PDF_INSPECTION` | Classification, confidence, title, page dimensions, OCR reasons |
| `PDF_MARKDOWN` | Document and page Markdown, incorporating requested OCR fusion |
| `PDF_TEXT` | Plain native text grouped into lines, per page and document |
| `PDF_ITEMS` | Native positioned runs, including font family/tag, weight class, bold provenance, fixed pitch, styles, links, MCID, rotation, advance availability, baseline shift, and legacy symbol-rewrite provenance |
| `PDF_STRUCTURE` | Tagged structure references joined to items through `(page, mcid)` |
| `PDF_GEOMETRY` | Native path rectangles and line segments |
| `PDF_RENDER` | Page pixels, dimensions, stride, format, and coordinate transforms |
| `PDF_ANALYSIS` | Native layout/quality assessment, OCR reasons, and per-page encoding-issue flags, without text or Markdown output |
| `PDF_TABLES` | Automatically detected native data tables with logical cell matrices, plus any explicit table queries |

Inspection accompanies every execution. Native text/items preserve the PDF's extraction evidence. Page provenance describes final Markdown. Page flags distinguish native OCR recommendations, actual recognition, tables, columns, and hosted-processing recommendations. Table flags incorporate tables found in final OCR Markdown as well as native layout evidence.

`PDF_ANALYSIS` is also marked present when Markdown or OCR processing requires it. When absent, unset layout/quality flags mean unassessed, not a clean bill of health. `PDF_PAGE_ENCODING_ISSUES` identifies pages whose OCR reasons report suspected garbled text; the document's `has_encoding_issues` aggregates selected pages. Analysis-only requests publish neither Markdown nor text.

The document Markdown is the selected pages' Markdown in order, separated by blank lines, with `<!-- Page N -->` markers when `PDF_MD_PAGE_NUMBERS` is set. Native OCR returns the core pipeline's document Markdown instead.

```text
PdfResult
+-- PdfResultView
    +-- document metadata and text views
    +-- pages[]
    |   +-- items[], structure[], rectangles[], lines[]
    |   +-- OCR reasons, provenance, warnings
    |   +-- optional rendered image
    +-- regions[]
    +-- tables[] -> cells[]
    +-- structure_nodes[] -> content references[]
```

`present` contains the available requested projections. NULL views denote absence; non-NULL views with length zero denote present empty data. Strings are UTF-8, are not NUL-terminated, and can contain embedded NULs. Use their lengths rather than `strlen`. Array indices and table row/column indices are zero-based; page numbers are always 1-based.

## Regions, tables, and composition

`request.regions` batches text, table, and vector-grid queries. Each descriptor supplies its own page and rectangle; this batch is independent of the output page selection and preserves descriptor order. Text and table queries are executed as one core call per kind. A text/table result includes text and OCR-routing information. A grid result has `PDF_REGION_GRID_FOUND` only when a reliable grid was detected, with structure tokens and cell boxes. No grid is a successful empty result.

`request.tables` accepts TSR structure-token arrays and cell quadrilaterals. Tokens use the upstream TSR vocabulary, including `<td></td>` or split `<td`, attribute, `>` tokens. Before the core's lenient parser sees them, the adapter requires one cell quadrilateral per cell tag, integer `rowspan`/`colspan` values, `colspan` within the core's 25-column limit, and `rowspan` within the declared row count. `PDF_TSR_STRICT` returns resolved cells and their Markdown. `PDF_TSR_AUTO` applies upstream quality repair and heuristic fallback, reporting its reason. When a fallback or repair changes the cells, the cell array is empty rather than describing a different table from the returned Markdown. All hinted tables are resolved with at most two core calls.

With `PDF_TABLES`, `result.tables` starts with automatically detected native data tables for the selected pages. TOCs are excluded. These tables contain the detector's logical cell matrix and its formatted Markdown; formatting may clean empty rows or cells. The detector does not retain a reliable cell-box or merged-span contract: automatic results leave `PDF_TABLE_HAS_BOUNDS`, `PDF_CELL_HAS_BOUNDS`, and `PDF_CELL_SPAN_KNOWN` unset. Their matrix entries use unit spans and do not claim header semantics. Do not interpret zero bounds as measured geometry.

Explicit table-query results follow automatic detections in input order, carry `PDF_TABLE_FROM_HINT`, and identify their descriptor through `input_index`. Their table bounds and resolved cell geometry/spans are marked available. Explicit queries remain independent of the output page selection and run even without `PDF_TABLES`; the result marks that collection present. If both automatic detection and explicit queries cover the same table, both pieces of evidence are returned, distinguished by origin. The automatic collection is native extraction evidence; OCR-discovered table presence is reflected in page flags, not synthesized native cells.

`pdf_inspector_compose` accepts a `PdfComposeInput`, initialized with `pdf_inspector_compose_init`. Select plain UTF-8 text or a batch of positioned items. Item composition requires page dimensions and accepts rectangles, lines, and structure-role references. It returns an independently owned Markdown result and never modifies input items.

## Semantic structure

`PDF_STRUCTURE` returns both per-page MCID/role associations for item joins and composition, and `result.structure_nodes` for semantic hierarchy. Nodes preserve parsed child order in preorder; their 1-based IDs are local to the result, and parent 0 denotes a root. Nodes retain role, alternative text, actual-text override, language override, and direct `(page, mcid)` references. Ancestors of selected-page nodes are retained. References to unselected known pages are omitted; unresolved references use page 0, and unscoped semantic leaves are preserved. Untagged PDFs return a present empty node array.

This exposes the structure the core parser recovered. It does not invent tags for untagged documents or reconstruct interleaving that the core's separate child/reference collections do not preserve.

## Coordinates

`PdfRequest.frame` selects the coordinate frame for page dimensions, positioned runs, path geometry, and region rects. `PDF_FRAME_SHEET` (default) uses PDF points relative to the unrotated visible page box, with a top-left origin and Y increasing downward. The visible box is the intersection of CropBox and MediaBox, using upstream fallbacks for missing or invalid boxes. Width and height describe that unrotated box; `PdfPageInfo.rotation` reports the inherited PDF `/Rotate` value separately.

`PDF_FRAME_DISPLAY` reports the same geometry on the rendered page instead: the visible box turned clockwise by the inheritable `/Rotate`, top-left origin, Y down, with dimensions swapped by a quarter turn. Items sit where a renderer draws them, `rotation` reads `0` for text that renders horizontally (sheet rotation plus `/Rotate`), and region rects can be taken straight from a page image. Unknown frame values are rejected.

`PdfRequest.bold_from_weight` is the same opt-in as the core positioned-text APIs: `0` (default) leaves `is_bold` and item merging unchanged; `1` also treats a weight class at or above `bold_weight_threshold` (600 by default, valid 100..=900) as bold. The threshold is validated even when the option is off. Markdown and native OCR ignore these knobs.

Each item reports `font_weight` (`0` when unknown, otherwise 100..=900), `bold_source` (`0` when not bold, otherwise `PDF_BOLD_FONT_NAME`, `PDF_BOLD_FONT_FLAGS`, `PDF_BOLD_WEIGHT_CLASS`, or `PDF_BOLD_PAINTED`), and `fixed_pitch` (`PDF_PITCH_UNKNOWN`, `PDF_PITCH_FIXED`, or `PDF_PITCH_PROPORTIONAL`). These are extraction evidence, not Markdown styling.

Text, region, rectangle, and line geometry all follow the request frame. Plain-text content is frame-independent and grouped in sheet order, so the frame never changes line breaks. Text rotation is clockwise in `[0,360)`; positive `baseline_shift` means raised text, independently of the direction of the Y axis. `PDF_ADVANCE_KNOWN` distinguishes measured text advances from estimates. `PDF_LEGACY_SYMBOL_REWRITE` marks runs whose decoded text includes a character changed by legacy symbol cleanup; it is decoding provenance, not an OCR verdict, and its absence is not a guarantee of decoding accuracy.

Table structure inputs, external OCR spans, and rendered-image transforms remain in the sheet frame.

Images use top-left pixel coordinates. Each image exposes affine maps in both directions, including PDF rotation and page-box offsets:

```text
x' = a*x + c*y + e
y' = b*x + d*y + f
```

Apply these transforms to image-space model output before supplying OCR or table geometry. Region and table rectangles require ordered corners and positive area. Positioned runs may have zero extent when the source reports it.

## Rendering and OCR

Rendering options control DPI, RGB/RGBA/gray pixels, annotations, form fields, and the maximum number of bytes per page. Select page batches to bound retained image memory. PDFium is loaded once per process and shared by every document.

Native OCR modes are `PDF_OCR_OFF`, `PDF_OCR_AUTO`, and `PDF_OCR_FORCE`. Auto uses upstream routing, native recovery, and fusion; Force recognizes all selected pages. Runtime library discovery follows `PDFIUM_LIB_PATH`, `ORT_DYLIB_PATH`, and upstream platform search behavior. Model options expose an explicit directory, confidence thresholds, and `PDF_DOWNLOAD_IF_MISSING` or `PDF_DOWNLOAD_OFFLINE`. Native processing remains lazy; OCR off performs no inference or model acquisition. Native OCR pages report the core pipeline's provenance and timings; recognition span geometry is not published.

External OCR supplies one `PdfOcrPageInput` per recognized page, with page-space quadrilaterals, UTF-8 spans, confidence, model identity, and optional warnings/timings. Pages must be unique and belong to the output selection. Supplied recognition replaces native recognition for those pages and goes through the core's public fusion (`fuse_ocr_pages`): full-page assembly with ordinary deduplication, weak-OCR warnings, and hosted-pipeline recommendations. It requires no native renderer, model files, or pixel buffers; the adapter presents each page to the core as a 72 dpi frame whose pixel coordinates equal page points. Span confidence is always supplied; `PDF_HAS_ORIENTATION` marks optional span orientation. `PDF_HAS_CONFIDENCE` marks optional page mean confidence. External recognition has no known render DPI, so its provenance reports 0.

## Runtime preparation

`pdf_inspector_capabilities()` reports compiled support. To verify actual native readiness before opening a PDF, initialize `PdfRuntimeOptions` with `pdf_inspector_runtime_options_init` and call `pdf_inspector_prepare_runtime`. NULL options request rendering and OCR with downloads disabled. Select `PDF_CAP_RENDER`, `PDF_CAP_OCR`, or both; OCR also checks rendering. Downloads require explicit `PDF_DOWNLOAD_IF_MISSING`. Explicit model directories are verified and never supplemented from the network.

Successful preparation returns an ordinary owned result with `present == PDF_RUNTIME`, `runtime.ready`, and the OCR model identity when requested. The OCR sessions are the same process-cached sessions used by document processing, and the loaded PDFium handle is the one later rendering uses. Rendering readiness means PDFium loaded successfully, not that a particular PDF can be rendered. Errors use the ordinary owned diagnostic and publish no partial result; free preparation results with `pdf_inspector_result_free`. `PDF_RUNTIME` is not a valid document-request output flag.

## Ownership and errors

- Document, result, and error handles have distinct lifetimes. Free each exactly once with its matching function. NULL free is a no-op.
- Every result owns all of its nested arrays, strings, and pixels. Subsequent calls and document destruction do not invalidate it. Freeing the result invalidates every view obtained from it.
- Requests and their nested arrays are borrowed only during the synchronous call. Input memory must remain readable and unchanged until it returns.
- Output slots must be writable, distinct from each other and inputs, and contain no unreleased handle. Calls clear those slots before validation. A failure publishes no partial result.
- Fallible operations return a status and optionally an independently owned error. Pass NULL for the error slot to discard diagnostics. Other calls cannot replace or invalidate an error's message.
- A document is immutable after open. Any number of executions may run against it concurrently from any threads, subject only to backend synchronization inside PDFium or ONNX Runtime. Result views support concurrent reads. Freeing a handle must not race with calls or borrowed reads.
- Non-NULL pointers must be aligned, live, and valid for their declared lengths/types. Invalid or previously freed addresses are outside the interface contract.

Rust panics are caught at fallible boundaries and reported as `PDF_PANIC`. This requires Rust's unwind panic runtime. Process-aborting failures, including allocator exhaustion or a build configured with `panic=abort`, cannot be converted to returned errors.
