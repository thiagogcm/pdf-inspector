#ifndef PDF_INSPECTOR_H
#define PDF_INSPECTOR_H

#include <stddef.h>
#include <stdint.h>

#define PDF_OK 0

#define PDF_INVALID_ARGUMENT 1

#define PDF_IO_ERROR 2

#define PDF_PASSWORD_ERROR 3

#define PDF_PARSE_ERROR 4

#define PDF_UNSUPPORTED 5

#define PDF_RUNTIME_ERROR 6

#define PDF_PANIC 7

#define PDF_SOURCE_BYTES 0

#define PDF_SOURCE_PATH 1

#define PDF_INSPECTION 1

#define PDF_MARKDOWN 2

#define PDF_TEXT 4

#define PDF_ITEMS 8

#define PDF_STRUCTURE 16

#define PDF_GEOMETRY 32

#define PDF_RENDER 64

#define PDF_ANALYSIS 128

#define PDF_TABLES 256

/**
 * Result-only projection returned by runtime preparation.
 */
#define PDF_RUNTIME 512

/**
 * Coordinate frame for page-space geometry: the visible page box as laid out
 * in the content stream (`/Rotate` not applied).
 */
#define PDF_FRAME_SHEET 0

/**
 * Rendered-page frame: the visible box turned clockwise by the inheritable
 * `/Rotate`, so region rects can be taken from a page image and items sit
 * where a renderer draws them.
 */
#define PDF_FRAME_DISPLAY 1

/**
 * `PdfItem.bold_source` when `is_bold` came from a bold word or foundry
 * abbreviation in the font name.
 */
#define PDF_BOLD_FONT_NAME 1

/**
 * `PdfItem.bold_source` when the FontDescriptor ForceBold flag or the
 * embedded program's bold selection said bold.
 */
#define PDF_BOLD_FONT_FLAGS 2

/**
 * `PdfItem.bold_source` when `bold_from_weight` credited the weight class.
 */
#define PDF_BOLD_WEIGHT_CLASS 3

/**
 * `PdfItem.bold_source` when the run was filled and stroked to look heavier.
 */
#define PDF_BOLD_PAINTED 4

/**
 * `PdfItem.fixed_pitch` when the face does not declare or measure a pitch.
 */
#define PDF_PITCH_UNKNOWN 0

/**
 * `PdfItem.fixed_pitch` when the face is monospaced.
 */
#define PDF_PITCH_FIXED 1

/**
 * `PdfItem.fixed_pitch` when the face is proportional.
 */
#define PDF_PITCH_PROPORTIONAL 2

#define PDF_CAP_RENDER 1

#define PDF_CAP_OCR 2

#define PDF_CAP_DOWNLOAD 4

#define PDF_CAP_EXTERNAL_OCR 8

#define PDF_OCR_OFF 0

#define PDF_OCR_AUTO 1

#define PDF_OCR_FORCE 2

#define PDF_DOWNLOAD_IF_MISSING 0

#define PDF_DOWNLOAD_OFFLINE 1

#define PDF_RGB8 0

#define PDF_RGBA8 1

#define PDF_GRAY8 2

#define PDF_REGION_TEXT 0

#define PDF_REGION_TABLE 1

#define PDF_REGION_GRID 2

#define PDF_TSR_AUTO 0

#define PDF_TSR_STRICT 1

#define PDF_COMPOSE_TEXT 0

#define PDF_COMPOSE_ITEMS 1

#define PDF_TYPE_TEXT 0

#define PDF_TYPE_SCANNED 1

#define PDF_TYPE_IMAGE 2

#define PDF_TYPE_MIXED 3

#define PDF_SCAN_SAMPLE 0

#define PDF_SCAN_FULL 1

#define PDF_SCAN_EARLY_EXIT 2

#define PDF_SCAN_PAGES 3

#define PDF_PROFILE_COMPACT 0

#define PDF_PROFILE_FIDELITY 1

#define PDF_ITEM_TEXT 0

#define PDF_ITEM_IMAGE 1

#define PDF_ITEM_LINK 2

#define PDF_ITEM_FORM_FIELD 3

#define PDF_BOLD 1

#define PDF_ITALIC 2

#define PDF_UNDERLINE 4

#define PDF_STRIKEOUT 8

#define PDF_HAS_MCID 16

#define PDF_ADVANCE_KNOWN 32

/**
 * Decoding provenance: legacy private-use symbol cleanup changed a character.
 * Absence does not guarantee decoding accuracy.
 */
#define PDF_LEGACY_SYMBOL_REWRITE 64

#define PDF_PAGE_NEEDS_OCR 1

#define PDF_PAGE_HAS_TABLES 2

#define PDF_PAGE_HAS_COLUMNS 4

#define PDF_PAGE_OCR_RAN 8

#define PDF_PAGE_HOSTED_RECOMMENDED 16

#define PDF_PAGE_ENCODING_ISSUES 32

#define PDF_PAGE_GID_ENCODED 64

#define PDF_PAGE_SKIPPED_INVISIBLE 128

#define PDF_PAGE_NATIVE_RECOVERED 256

#define PDF_READING_SINGLE 0

#define PDF_READING_TABULAR 1

#define PDF_READING_NEWSPAPER 2

#define PDF_LOAD_DECRYPTED 1

#define PDF_LOAD_WIDENED_FORM_BBOX 2

#define PDF_LOAD_LEADING_BYTES 4

#define PDF_LOAD_CONTAINER_REPAIRED 8

#define PDF_TABLE_DATA 0

#define PDF_TABLE_TOC 1

#define PDF_CONTENT_NATIVE 0

#define PDF_CONTENT_OCR 1

#define PDF_CONTENT_FUSED 2

#define PDF_HAS_CONFIDENCE 1

#define PDF_HAS_ORIENTATION 2

#define PDF_REGION_NEEDS_OCR 1

#define PDF_REGION_GRID_FOUND 2

#define PDF_CELL_HEADER 1

#define PDF_CELL_HAS_BOUNDS 2

#define PDF_CELL_SPAN_KNOWN 4

#define PDF_TABLE_FROM_HINT 1

#define PDF_TABLE_HAS_BOUNDS 2

#define PDF_MD_HEADERS 1

#define PDF_MD_LISTS 2

#define PDF_MD_CODE 4

#define PDF_MD_REMOVE_PAGE_NUMBERS 8

#define PDF_MD_URLS 16

#define PDF_MD_HYPHENATION 32

#define PDF_MD_BOLD 64

#define PDF_MD_ITALIC 128

#define PDF_MD_UNDERLINE 256

#define PDF_MD_IMAGES 512

#define PDF_MD_LINKS 1024

#define PDF_MD_PAGE_NUMBERS 2048

#define PDF_MD_STRIP_FURNITURE 4096

/**
 * Owns decrypted document bytes and page geometry. Immutable after open, so
 * any number of executions may run against it concurrently.
 */
typedef struct PdfDocument PdfDocument;

/**
 * Owns the diagnostic for a single failed call.
 */
typedef struct PdfErrorHandle PdfErrorHandle;

/**
 * Owns a result and all memory reachable through its view.
 */
typedef struct PdfResult PdfResult;

/**
 * UTF-8 text or binary bytes, never NUL-terminated. NULL/0 denotes absence;
 * non-NULL/0 denotes a present empty value. Output storage belongs to its result.
 */
typedef struct {
  const uint8_t *ptr;
  size_t len;
} PdfBytes;

/**
 * Prepare selected native capabilities without a PDF. Initialized policy is offline.
 */
typedef struct {
  uint32_t capabilities;
  uint32_t download_policy;
  PdfBytes model_directory;
} PdfRuntimeOptions;

typedef struct {
  PdfBytes password;
} PdfOpenOptions;

typedef struct {
  const uint32_t *ptr;
  size_t len;
} PdfPageNumbers;

typedef struct {
  uint32_t flags;
  uint32_t profile;
  float base_font_size;
} PdfMarkdownOptions;

typedef struct {
  uint32_t strategy;
  uint32_t sample_size;
  uint32_t min_text_ops;
  float text_page_ratio;
  PdfPageNumbers pages;
} PdfDetectionOptions;

typedef struct {
  float dpi;
  uint32_t format;
  uint32_t annotations;
  uint32_t form_fields;
  uint64_t max_page_bytes;
} PdfRenderOptions;

typedef struct {
  uint32_t mode;
  uint32_t download_policy;
  float minimum_confidence;
  float hosted_confidence;
  PdfBytes model_directory;
} PdfOcrOptions;

/**
 * Axis-aligned bounds in the request's frame (`PDF_FRAME_*`), top-left origin.
 */
typedef struct {
  float x0;
  float y0;
  float x1;
  float y1;
} PdfBox;

typedef struct {
  uint32_t page;
  uint32_t kind;
  PdfBox bounds;
} PdfRegionInput;

typedef struct {
  const PdfRegionInput *ptr;
  size_t len;
} PdfRegionInputs;

typedef struct {
  const PdfBytes *ptr;
  size_t len;
} PdfStrings;

typedef struct {
  float x;
  float y;
} PdfPoint;

/**
 * Cell quadrilateral in page points; four corners in perimeter order.
 */
typedef struct {
  PdfPoint points[4];
} PdfQuad;

typedef struct {
  const PdfQuad *ptr;
  size_t len;
} PdfQuads;

typedef struct {
  uint32_t page;
  uint32_t mode;
  PdfBox bounds;
  PdfStrings tokens;
  PdfQuads cells;
} PdfTableInput;

typedef struct {
  const PdfTableInput *ptr;
  size_t len;
} PdfTableInputs;

typedef struct {
  PdfBytes text;
  PdfQuad polygon;
  float confidence;
  float orientation;
  uint32_t flags;
} PdfOcrSpan;

typedef struct {
  const PdfOcrSpan *ptr;
  size_t len;
} PdfOcrSpans;

typedef struct {
  uint32_t page;
  uint32_t flags;
  float confidence;
  uint64_t processing_ms;
  PdfBytes model;
  PdfBytes model_revision;
  PdfStrings warnings;
  PdfOcrSpans spans;
} PdfOcrPageInput;

typedef struct {
  const PdfOcrPageInput *ptr;
  size_t len;
} PdfOcrPageInputs;

/**
 * Initialize with pdf_inspector_request_init. Empty page selection means all
 * pages. Page lists are sets; query batches preserve input order. `frame`
 * selects the coordinate frame for page dimensions, positioned runs,
 * path geometry, and region rects (see `PDF_FRAME_*`). `bold_from_weight`
 * is 0 or 1; `bold_weight_threshold` is the 100..=900 class that option
 * treats as bold (600 by default). `include_invisible` is 0 or 1.
 */
typedef struct {
  uint32_t outputs;
  PdfPageNumbers pages;
  PdfMarkdownOptions markdown;
  PdfDetectionOptions detection;
  PdfRenderOptions render;
  PdfOcrOptions ocr;
  PdfRegionInputs regions;
  PdfTableInputs tables;
  PdfOcrPageInputs external_ocr;
  uint32_t frame;
  uint32_t bold_from_weight;
  uint32_t bold_weight_threshold;
  uint32_t include_invisible;
} PdfRequest;

typedef struct {
  uint32_t page;
  float width;
  float height;
  uint32_t rotation;
} PdfPageInfo;

typedef struct {
  const PdfPageInfo *ptr;
  size_t len;
} PdfPageInfos;

/**
 * Complete positioned run. Rotation is clockwise; positive baseline_shift
 * denotes superscript. MCID is meaningful only when PDF_HAS_MCID is set.
 * PDF_LEGACY_SYMBOL_REWRITE is decoding provenance, not an OCR verdict.
 * `font_weight` is 0 when unknown, else 100..=900. `bold_source` is 0 when
 * `is_bold` is unset, else `PDF_BOLD_FONT_*` / `PDF_BOLD_PAINTED`.
 * `fixed_pitch` is `PDF_PITCH_*`. `dest_page` is a 1-indexed GoTo target;
 * 0 means none. URI stays in `link`.
 */
typedef struct {
  uint32_t page;
  uint32_t kind;
  uint32_t flags;
  PdfBox bounds;
  float font_size;
  float rotation;
  float baseline_shift;
  int64_t mcid;
  uint32_t font_weight;
  uint32_t bold_source;
  uint32_t fixed_pitch;
  uint32_t dest_page;
  PdfBytes text;
  PdfBytes font;
  PdfBytes font_tag;
  PdfBytes link;
} PdfItem;

typedef struct {
  const PdfItem *ptr;
  size_t len;
} PdfItems;

typedef struct {
  uint32_t page;
  PdfBox bounds;
} PdfRectangle;

typedef struct {
  const PdfRectangle *ptr;
  size_t len;
} PdfRectangles;

typedef struct {
  uint32_t page;
  PdfPoint start;
  PdfPoint end;
} PdfSegment;

typedef struct {
  const PdfSegment *ptr;
  size_t len;
} PdfSegments;

typedef struct {
  uint32_t page;
  int64_t mcid;
  PdfBytes role;
} PdfStructureElement;

typedef struct {
  const PdfStructureElement *ptr;
  size_t len;
} PdfStructureElements;

/**
 * Caller-owned composition input. Page dimensions are required for items.
 */
typedef struct {
  uint32_t kind;
  PdfBytes text;
  PdfPageInfos pages;
  PdfItems items;
  PdfRectangles rectangles;
  PdfSegments lines;
  PdfStructureElements structure;
  PdfMarkdownOptions markdown;
} PdfComposeInput;

typedef struct {
  uint32_t kind;
  PdfBytes data;
} PdfSource;

/**
 * Native-layer text quality numbers. `density` is alphanumeric/visible
 * (`0` when there are no visible characters). `english_cosine` is 1 when
 * there are no ASCII letters. `score` is the 0–1 native-candidate formula.
 */
typedef struct {
  uint32_t alphanumeric_chars;
  uint32_t visible_chars;
  float density;
  uint32_t replacement_chars;
  uint32_t longest_replacement_run;
  float english_cosine;
  float score;
} PdfPageQuality;

/**
 * Horizontal column interval in the request frame; y is not invented.
 */
typedef struct {
  float x0;
  float x1;
} PdfInterval;

typedef struct {
  const PdfInterval *ptr;
  size_t len;
} PdfIntervals;

typedef struct {
  const PdfBox *ptr;
  size_t len;
} PdfBoxes;

/**
 * Affine mapping: x' = a*x + c*y + e; y' = b*x + d*y + f.
 */
typedef struct {
  double a;
  double b;
  double c;
  double d;
  double e;
  double f;
} PdfTransform;

typedef struct {
  uint32_t width;
  uint32_t height;
  size_t stride;
  uint32_t format;
  PdfBytes pixels;
  PdfTransform pixel_to_page;
  PdfTransform page_to_pixel;
} PdfImage;

typedef struct {
  uint32_t source;
  uint32_t flags;
  float confidence;
  float render_dpi;
  uint64_t render_ms;
  uint64_t ocr_ms;
  uint64_t assembly_ms;
  PdfBytes model;
  PdfBytes model_revision;
  PdfStrings warnings;
} PdfProvenance;

/**
 * `reading_order` is `PDF_READING_*`. `quality` is filled when native text
 * or items are produced. `columns` are x-only intervals in the request frame.
 * `charts` and `image_regions` are supplemental boxes in that frame.
 */
typedef struct {
  PdfPageInfo info;
  uint32_t flags;
  uint32_t reading_order;
  PdfPageQuality quality;
  PdfBytes markdown;
  PdfBytes text;
  PdfStrings ocr_reasons;
  PdfItems items;
  PdfIntervals columns;
  PdfBoxes charts;
  PdfBoxes image_regions;
  PdfStructureElements structure;
  PdfRectangles rectangles;
  PdfSegments lines;
  PdfImage image;
  PdfProvenance provenance;
} PdfPage;

typedef struct {
  const PdfPage *ptr;
  size_t len;
} PdfPages;

typedef struct {
  uint32_t page;
  uint32_t kind;
  uint32_t flags;
  PdfBox bounds;
  PdfBytes text;
  PdfBytes ocr_reason;
  PdfStrings tokens;
  PdfBoxes cells;
} PdfRegion;

typedef struct {
  const PdfRegion *ptr;
  size_t len;
} PdfRegions;

typedef struct {
  const float *ptr;
  size_t len;
} PdfFloats;

typedef struct {
  size_t row;
  size_t column;
  size_t row_span;
  size_t column_span;
  uint32_t flags;
  PdfBox bounds;
  PdfBytes text;
} PdfCell;

typedef struct {
  const PdfCell *ptr;
  size_t len;
} PdfCells;

/**
 * `kind` is `PDF_TABLE_DATA` or `PDF_TABLE_TOC`. `column_edges` / `row_edges`
 * are detector bands in the request frame when `PDF_TABLE_HAS_BOUNDS` is set.
 */
typedef struct {
  uint32_t page;
  uint32_t flags;
  /**
   * Index in request.tables for hinted results; meaningful only with PDF_TABLE_FROM_HINT.
   */
  size_t input_index;
  uint32_t kind;
  PdfBox bounds;
  PdfBytes markdown;
  PdfBytes fallback_reason;
  PdfFloats column_edges;
  PdfFloats row_edges;
  PdfCells cells;
} PdfTable;

typedef struct {
  const PdfTable *ptr;
  size_t len;
} PdfTables;

typedef struct {
  uint32_t page;
  int64_t mcid;
} PdfContentReference;

typedef struct {
  const PdfContentReference *ptr;
  size_t len;
} PdfContentReferences;

/**
 * A semantic node in preorder. IDs are 1-based; parent 0 denotes a root.
 * References with page 0 have an unresolved page in the source structure.
 */
typedef struct {
  size_t id;
  size_t parent;
  PdfBytes role;
  PdfBytes alt_text;
  PdfBytes actual_text;
  PdfBytes language;
  PdfContentReferences references;
} PdfStructureNode;

typedef struct {
  const PdfStructureNode *ptr;
  size_t len;
} PdfStructureNodes;

typedef struct {
  uint32_t ready;
  PdfBytes model;
  PdfBytes model_revision;
} PdfRuntimeInfo;

/**
 * Load-time repairs recorded at open. `leading_bytes` is the `%PDF-` offset.
 */
typedef struct {
  uint32_t flags;
  uint32_t leading_bytes;
  uint32_t widened_form_bboxes;
} PdfLoadAudit;

/**
 * One immutable graph of borrowed records. All nested storage lasts until
 * pdf_inspector_result_free, independently of the source document.
 * `ocr_recommended`, `pages_sampled`, and `pages_with_text` come from
 * detector inspection. `audit` is recorded at open.
 */
typedef struct {
  uint32_t present;
  uint32_t pdf_type;
  uint32_t page_count;
  float confidence;
  uint32_t has_encoding_issues;
  uint32_t ocr_recommended;
  uint32_t pages_sampled;
  uint32_t pages_with_text;
  uint64_t processing_ms;
  PdfBytes title;
  PdfBytes markdown;
  PdfBytes text;
  PdfPages pages;
  PdfRegions regions;
  PdfTables tables;
  PdfStructureNodes structure_nodes;
  PdfRuntimeInfo runtime;
  PdfLoadAudit audit;
} PdfResultView;

typedef struct {
  int32_t status;
  PdfBytes message;
} PdfDiagnostic;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Compiled capabilities. Does not load libraries, inspect models, or use the network.
 */
uint32_t pdf_inspector_capabilities(void);

/**
 * Initialize options. Returns PDF_INVALID_ARGUMENT for NULL.
 */
int32_t pdf_inspector_runtime_options_init(PdfRuntimeOptions *out);

/**
 * Verify native runtime readiness without a document, initializing reusable OCR sessions.
 * NULL options request rendering and OCR, offline. Downloads require explicit opt-in.
 * Returns a runtime result or an owned diagnostic; use the ordinary result/error functions.
 */
int32_t pdf_inspector_prepare_runtime(const PdfRuntimeOptions *options,
                                      PdfResult **out,
                                      PdfErrorHandle **error);

/**
 * Initialize options. Returns PDF_INVALID_ARGUMENT for NULL.
 */
int32_t pdf_inspector_open_options_init(PdfOpenOptions *out);

/**
 * Initialize a request to all pages, inspection and Markdown, with OCR off.
 */
int32_t pdf_inspector_request_init(PdfRequest *out);

/**
 * Initialize composition options for plain UTF-8 text.
 */
int32_t pdf_inspector_compose_init(PdfComposeInput *out);

/**
 * Open bytes or a UTF-8 path. Source and password memory may be released on return.
 * NULL options use the default empty-password behavior.
 */
int32_t pdf_inspector_open(const PdfSource *source,
                           const PdfOpenOptions *options,
                           PdfDocument **out,
                           PdfErrorHandle **error);

/**
 * Execute against an open document. NULL request uses initialized defaults.
 * Each call publishes an independent result or an independent error.
 */
int32_t pdf_inspector_execute(const PdfDocument *document,
                              const PdfRequest *request,
                              PdfResult **out,
                              PdfErrorHandle **error);

/**
 * Compose Markdown from plain text or positioned items, without a document handle.
 */
int32_t pdf_inspector_compose(const PdfComposeInput *input,
                              PdfResult **out,
                              PdfErrorHandle **error);

/**
 * Borrow the immutable root view; NULL returns NULL.
 */
const PdfResultView *pdf_inspector_result_view(const PdfResult *result);

/**
 * Borrow a diagnostic; NULL returns NULL. Other calls cannot invalidate it.
 */
const PdfDiagnostic *pdf_inspector_error_view(const PdfErrorHandle *error);

/**
 * Release a document. Existing results remain valid.
 */
void pdf_inspector_document_free(PdfDocument *document);

/**
 * Release a result and invalidate all views obtained from it.
 */
void pdf_inspector_result_free(PdfResult *result);

/**
 * Release an error and invalidate its diagnostic view.
 */
void pdf_inspector_error_free(PdfErrorHandle *error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* PDF_INSPECTOR_H */
