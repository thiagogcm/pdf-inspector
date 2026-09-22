#ifndef PDF_INSPECTOR_H
#define PDF_INSPECTOR_H

#include <stddef.h>
#include <stdint.h>

/**
 * Status codes returned by fallible calls and carried in `PdfError.status`.
 */
#define PDF_OK 0

#define PDF_INVALID_ARGUMENT 1

#define PDF_IO_ERROR 2

#define PDF_PASSWORD_ERROR 3

#define PDF_PARSE_ERROR 4

#define PDF_UNSUPPORTED 5

#define PDF_RUNTIME_ERROR 6

#define PDF_PANIC 7

/**
 * `PdfSource.kind`.
 */
#define PDF_SOURCE_BYTES 0

#define PDF_SOURCE_PATH 1

/**
 * `PdfRequest.outputs` and `PdfResult.present` bits.
 */
#define PDF_OUT_INSPECTION 1

#define PDF_OUT_MARKDOWN 2

#define PDF_OUT_TEXT 4

#define PDF_OUT_ITEMS 8

#define PDF_OUT_STRUCTURE 16

#define PDF_OUT_GEOMETRY 32

#define PDF_OUT_RENDER 64

#define PDF_OUT_ANALYSIS 128

#define PDF_OUT_TABLES 256

/**
 * `PdfRequest.flags`: also treat a weight class at or above
 * `bold_weight_threshold` as bold.
 */
#define PDF_REQUEST_BOLD_FROM_WEIGHT 1

/**
 * `PdfRequest.flags`: keep text drawn with render mode 3 (invisible).
 */
#define PDF_REQUEST_INCLUDE_INVISIBLE 2

/**
 * `PdfRenderOptions.flags` bits.
 */
#define PDF_RENDER_ANNOTATIONS 1

#define PDF_RENDER_FORM_FIELDS 2

/**
 * `PdfResult.flags`: a selected page reported suspected garbled text.
 */
#define PDF_DOC_ENCODING_ISSUES 1

/**
 * `PdfResult.flags`: the detector recommends OCR for the document.
 */
#define PDF_DOC_OCR_RECOMMENDED 2

/**
 * `PdfRequest.frame`: the visible page box as laid out in the content
 * stream (`/Rotate` not applied).
 */
#define PDF_FRAME_SHEET 0

/**
 * `PdfRequest.frame`: the visible box turned clockwise by the inheritable
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

/**
 * Capability bits: `pdf_inspector_capabilities()`, `PdfRuntimeOptions.capabilities`,
 * and `PdfRuntimeInfo.capabilities`.
 */
#define PDF_CAP_RENDER 1

#define PDF_CAP_OCR 2

#define PDF_CAP_DOWNLOAD 4

#define PDF_CAP_EXTERNAL_OCR 8

/**
 * `PdfOcrOptions.mode`.
 */
#define PDF_OCR_OFF 0

#define PDF_OCR_AUTO 1

#define PDF_OCR_FORCE 2

/**
 * `PdfOcrOptions.download_policy` and `PdfRuntimeOptions.download_policy`.
 */
#define PDF_DOWNLOAD_IF_MISSING 0

#define PDF_DOWNLOAD_OFFLINE 1

/**
 * `PdfRenderOptions.format` and `PdfImage.format`.
 */
#define PDF_RGB8 0

#define PDF_RGBA8 1

#define PDF_GRAY8 2

/**
 * `PdfRegionInput.kind` and `PdfRegion.kind`.
 */
#define PDF_REGION_TEXT 0

#define PDF_REGION_TABLE 1

#define PDF_REGION_GRID 2

/**
 * `PdfTableInput.mode`.
 */
#define PDF_TSR_AUTO 0

#define PDF_TSR_STRICT 1

/**
 * `PdfResult.pdf_type`.
 */
#define PDF_TYPE_TEXT 0

#define PDF_TYPE_SCANNED 1

#define PDF_TYPE_IMAGE 2

#define PDF_TYPE_MIXED 3

/**
 * `PdfDetectionOptions.strategy`.
 */
#define PDF_SCAN_SAMPLE 0

#define PDF_SCAN_FULL 1

#define PDF_SCAN_EARLY_EXIT 2

#define PDF_SCAN_PAGES 3

/**
 * `PdfMarkdownOptions.profile`.
 */
#define PDF_PROFILE_COMPACT 0

#define PDF_PROFILE_FIDELITY 1

/**
 * `PdfItem.kind`.
 */
#define PDF_ITEM_TEXT 0

#define PDF_ITEM_IMAGE 1

#define PDF_ITEM_LINK 2

#define PDF_ITEM_FORM_FIELD 3

/**
 * `PdfItem.flags` bits.
 */
#define PDF_BOLD 1

#define PDF_ITALIC 2

#define PDF_UNDERLINE 4

#define PDF_STRIKEOUT 8

#define PDF_HAS_MCID 16

#define PDF_ADVANCE_KNOWN 32

/**
 * `PdfItem.flags`: legacy private-use symbol cleanup changed a character.
 * Decoding provenance; absence does not guarantee decoding accuracy.
 */
#define PDF_LEGACY_SYMBOL_REWRITE 64

/**
 * `PdfPage.flags` bits.
 */
#define PDF_PAGE_NEEDS_OCR 1

#define PDF_PAGE_HAS_TABLES 2

#define PDF_PAGE_HAS_COLUMNS 4

#define PDF_PAGE_OCR_RAN 8

#define PDF_PAGE_HOSTED_RECOMMENDED 16

#define PDF_PAGE_ENCODING_ISSUES 32

#define PDF_PAGE_GID_ENCODED 64

#define PDF_PAGE_SKIPPED_INVISIBLE 128

#define PDF_PAGE_NATIVE_RECOVERED 256

/**
 * `PdfPage.reading_order`.
 */
#define PDF_READING_SINGLE 0

#define PDF_READING_TABULAR 1

#define PDF_READING_NEWSPAPER 2

/**
 * `PdfPage.text_orientation`: no positioned content was parsed.
 */
#define PDF_ORIENTATION_UNKNOWN 0

/**
 * `PdfPage.text_orientation`: text reads along +x.
 */
#define PDF_ORIENTATION_UPRIGHT 1

/**
 * `PdfPage.text_orientation`: most runs read bottom-to-top.
 */
#define PDF_ORIENTATION_CCW 2

/**
 * `PdfPage.text_orientation`: most runs read top-to-bottom.
 */
#define PDF_ORIENTATION_CW 3

/**
 * `PdfLoadAudit.flags` bits.
 */
#define PDF_LOAD_DECRYPTED 1

#define PDF_LOAD_WIDENED_FORM_BBOX 2

#define PDF_LOAD_LEADING_BYTES 4

#define PDF_LOAD_CONTAINER_REPAIRED 8

#define PDF_LOAD_SATURATED_BBOX 16

/**
 * `PdfTable.kind`.
 */
#define PDF_TABLE_DATA 0

#define PDF_TABLE_TOC 1

/**
 * `PdfProvenance.source`.
 */
#define PDF_CONTENT_NATIVE 0

#define PDF_CONTENT_OCR 1

#define PDF_CONTENT_FUSED 2

/**
 * Optional-field bits. `PdfOcrPageInput.flags` and `PdfProvenance.flags`
 * take `PDF_HAS_CONFIDENCE` only; `PdfOcrSpan.flags` takes
 * `PDF_HAS_ORIENTATION` only.
 */
#define PDF_HAS_CONFIDENCE 1

#define PDF_HAS_ORIENTATION 2

/**
 * `PdfRegion.flags` bits.
 */
#define PDF_REGION_NEEDS_OCR 1

#define PDF_REGION_GRID_FOUND 2

/**
 * `PdfCell.flags` bits.
 */
#define PDF_CELL_HEADER 1

#define PDF_CELL_HAS_BOUNDS 2

#define PDF_CELL_SPAN_KNOWN 4

/**
 * `PdfTable.flags` bits.
 */
#define PDF_TABLE_FROM_HINT 1

#define PDF_TABLE_HAS_BOUNDS 2

/**
 * `PdfMarkdownOptions.flags` bits.
 */
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
 * UTF-8 text or binary bytes, never NUL-terminated. NULL/0 denotes absence;
 * non-NULL/0 denotes a present empty value. Output storage belongs to its
 * result, except where a function documents static storage.
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

/**
 * Written by `pdf_inspector_prepare_runtime`. `capabilities` are the
 * verified `PDF_CAP_*` bits; model identity is filled when OCR was prepared.
 */
typedef struct {
  uint32_t capabilities;
  PdfBytes model;
  PdfBytes model_revision;
} PdfRuntimeInfo;

/**
 * Published by pointer for a failed call; release with pdf_inspector_error_free.
 */
typedef struct {
  int32_t status;
  PdfBytes message;
} PdfError;

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
  uint32_t flags;
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
 * path geometry, and region rects (see `PDF_FRAME_*`). `flags` are
 * `PDF_REQUEST_*`; `bold_weight_threshold` is the 100..=900 class that
 * `PDF_REQUEST_BOLD_FROM_WEIGHT` treats as bold (600 by default).
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
  uint32_t flags;
  uint32_t bold_weight_threshold;
} PdfRequest;

/**
 * `kind` is `PDF_SOURCE_*`. An absent password uses the loader's
 * empty-password behavior.
 */
typedef struct {
  uint32_t kind;
  PdfBytes data;
  PdfBytes password;
} PdfSource;

/**
 * Load-time repairs recorded at open. `leading_bytes` is the `%PDF-` offset;
 * `saturated_bbox_numerals` counts `/BBox` numerals repaired before parsing.
 */
typedef struct {
  uint32_t flags;
  uint32_t leading_bytes;
  uint32_t widened_form_bboxes;
  uint32_t saturated_bbox_numerals;
} PdfLoadAudit;

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
 * Facts recorded at open, borrowed from the document until
 * pdf_inspector_document_free. `pages` are sheet-frame dimensions of every page.
 */
typedef struct {
  uint32_t page_count;
  PdfLoadAudit audit;
  PdfPageInfos pages;
} PdfDocumentInfo;

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
 * Complete positioned run. Rotation is clockwise; positive baseline_shift
 * denotes superscript. MCID is meaningful only when PDF_HAS_MCID is set.
 * PDF_LEGACY_SYMBOL_REWRITE is decoding provenance, not an OCR verdict.
 * `font_weight` is 0 when unknown, else 100..=900. `bold_source` is 0 when
 * `is_bold` is unset, else `PDF_BOLD_FONT_*` / `PDF_BOLD_PAINTED`.
 * `fixed_pitch` is `PDF_PITCH_*`. `dest_page` is a 1-indexed GoTo target;
 * 0 means none. URI stays in `link`. Composition ignores `dest_page`: the
 * Markdown pipeline has no destination concept.
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

typedef struct {
  uint32_t page;
  int64_t mcid;
  PdfBytes role;
} PdfStructureElement;

typedef struct {
  const PdfStructureElement *ptr;
  size_t len;
} PdfStructureElements;

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
 * `reading_order` is `PDF_READING_*`; `text_orientation` is
 * `PDF_ORIENTATION_*`, assessed whenever positioned content is parsed
 * (items, text, geometry, or tables). `quality` is
 * filled when native text or items are produced. `columns` are x-only
 * intervals in the request frame. `charts` and `image_regions` are
 * supplemental boxes in that frame.
 */
typedef struct {
  PdfPageInfo info;
  uint32_t flags;
  uint32_t reading_order;
  uint32_t text_orientation;
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
  uint32_t row;
  uint32_t column;
  uint32_t row_span;
  uint32_t column_span;
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
  uint32_t input_index;
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
 * A semantic node in preorder. The node at index N has id N + 1; `parent`
 * is that id, 0 for a root. References with page 0 have an unresolved page
 * in the source structure.
 */
typedef struct {
  uint32_t parent;
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

/**
 * ToUnicode/CMap coverage observed while decoding positioned content.
 * `interpolated` codes were recovered from a one-code gap; `unmapped`
 * codes could not be decoded.
 */
typedef struct {
  PdfBytes font;
  uint32_t codes;
  uint32_t interpolated;
  uint32_t unmapped;
} PdfCMapGap;

typedef struct {
  const PdfCMapGap *ptr;
  size_t len;
} PdfCMapGaps;

/**
 * One immutable graph of records, published by pointer and released only
 * with pdf_inspector_result_free. Nested storage lasts as long as the
 * result, independently of the source document. `flags` are `PDF_DOC_*`;
 * `pages_sampled` and `pages_with_text` come from detector inspection;
 * `cmap_gaps` is populated when positioned content is parsed.
 */
typedef struct {
  uint32_t present;
  uint32_t pdf_type;
  uint32_t page_count;
  float confidence;
  uint32_t flags;
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
  PdfCMapGaps cmap_gaps;
} PdfResult;

/**
 * Caller-owned positioned content for `pdf_inspector_compose_items`.
 * Every referenced page needs an entry in `pages`.
 */
typedef struct {
  PdfPageInfos pages;
  PdfItems items;
  PdfRectangles rectangles;
  PdfSegments lines;
  PdfStructureElements structure;
} PdfComposeInput;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Library version as static UTF-8.
 */
PdfBytes pdf_inspector_version(void);

/**
 * Compiled capabilities. Does not load libraries, inspect models, or use the network.
 */
uint32_t pdf_inspector_capabilities(void);

/**
 * Initialize options. Returns PDF_INVALID_ARGUMENT for NULL.
 */
int32_t pdf_inspector_runtime_options_init(PdfRuntimeOptions *out);

/**
 * Verify native runtime readiness without a document, initializing reusable
 * OCR sessions. NULL options request rendering and OCR, offline. `out` is
 * zeroed first and written only on success.
 */
int32_t pdf_inspector_prepare_runtime(const PdfRuntimeOptions *options,
                                      PdfRuntimeInfo *out,
                                      PdfError **error);

/**
 * Initialize a request to all pages, inspection and Markdown, with OCR off.
 */
int32_t pdf_inspector_request_init(PdfRequest *out);

/**
 * Initialize Markdown options to the core defaults.
 */
int32_t pdf_inspector_markdown_options_init(PdfMarkdownOptions *out);

/**
 * Open bytes or a UTF-8 path. Source and password memory may be released on return.
 */
int32_t pdf_inspector_open(const PdfSource *source, PdfDocument **out, PdfError **error);

/**
 * Borrow page count, sheet-frame page dimensions, and the load audit; NULL returns NULL.
 */
const PdfDocumentInfo *pdf_inspector_document_info(const PdfDocument *document);

/**
 * Execute against an open document. NULL request uses initialized defaults.
 * Each call publishes an independent result or an independent error.
 */
int32_t pdf_inspector_execute(const PdfDocument *document,
                              const PdfRequest *request,
                              PdfResult **out,
                              PdfError **error);

/**
 * Compose Markdown from plain UTF-8 text. NULL options use the core defaults.
 */
int32_t pdf_inspector_compose_text(PdfBytes text,
                                   const PdfMarkdownOptions *options,
                                   PdfResult **out,
                                   PdfError **error);

/**
 * Compose Markdown from positioned items, without a document handle. NULL
 * options use the core defaults.
 */
int32_t pdf_inspector_compose_items(const PdfComposeInput *input,
                                    const PdfMarkdownOptions *options,
                                    PdfResult **out,
                                    PdfError **error);

/**
 * Release a document and its info. Existing results remain valid.
 */
void pdf_inspector_document_free(PdfDocument *document);

/**
 * Release a result and everything reachable from it.
 */
void pdf_inspector_result_free(PdfResult *result);

/**
 * Release an error and its message.
 */
void pdf_inspector_error_free(PdfError *error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* PDF_INSPECTOR_H */
