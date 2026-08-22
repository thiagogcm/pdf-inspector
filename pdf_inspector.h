#ifndef PDF_INSPECTOR_H
#define PDF_INSPECTOR_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
/* Processing configuration: mode, detection thresholds, Markdown formatting,
   and an optional page filter. */
typedef struct CPdfOptions CPdfOptions;

/* Result of a full processing run. */
typedef struct CPdfProcessResult CPdfProcessResult;

/* Lightweight classification result for routing decisions. */
typedef struct CPdfClassification CPdfClassification;

/* Full detector result, including sampling counts and per-page OCR reasons. */
typedef struct CPdfTypeResult CPdfTypeResult;

/* Per-page Markdown plus layout and OCR-routing metadata. */
typedef struct CPagesExtractionResult CPagesExtractionResult;

/* Extracted plain text, or converted Markdown. */
typedef struct CTextResult CTextResult;

/* Positioned text items. Also built by the caller, for the OCR round trip. */
typedef struct CTextItemsResult CTextItemsResult;

/* Structure-tree elements from a tagged PDF. */
typedef struct CStructureElementsResult CStructureElementsResult;

/* Region-based text or table extraction results. */
typedef struct CRegionTextResult CRegionTextResult;

/* Vector-grid detection result. A successful call always returns a handle,
   including when no grid was found: call
   pdf_inspector_vector_grid_result_is_detected() to tell the two apart. */
typedef struct CVectorGridResult CVectorGridResult;

/* Markdown tables recovered from external table-structure recognition. */
typedef struct CTsrTableExtractionResult CTsrTableExtractionResult;

/* Resolved TSR cells, one list per input descriptor, in input order. */
typedef struct CTsrStructuredCellsResult CTsrStructuredCellsResult;

/**
 * C ABI major version; bumped only for incompatible changes. See the
 * Versioning section in `docs/c-api.md`.
 */
#define PDF_INSPECTOR_ABI_VERSION 1

/**
 * C ABI minor version; bumped for additive, backward-compatible changes.
 * Resets to 0 on every major bump. See `docs/c-api.md`.
 */
#define PDF_INSPECTOR_ABI_MINOR 0

/**
 * `CTextItemDescriptor.flags` bits. Unknown bits are rejected with
 * `PdfInspectorError_InvalidArgument`.
 */
#define PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD (1 << 0)

#define PDF_INSPECTOR_TEXT_ITEM_FLAG_ITALIC (1 << 1)

#define PDF_INSPECTOR_TEXT_ITEM_FLAG_UNDERLINE (1 << 2)

#define PDF_INSPECTOR_TEXT_ITEM_FLAG_STRIKEOUT (1 << 3)

/**
 * When set, `CTextItemDescriptor.mcid` carries a marked-content ID; when
 * clear, `mcid` is ignored and the item has none.
 */
#define PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID (1 << 4)

/**
 * The machine-readable OCR reasons the reason getters emit, as a switchable
 * discriminant. Map a reason string to one with
 * `pdf_inspector_ocr_reason_from_string`; anything the running library emits
 * that this header predates maps to `Unknown`, so a consumer can fall back
 * to the raw bytes rather than mis-handling it.
 */
typedef enum {
  COcrReason_Unknown = -1,
  /**
   * The extracted text layer looks garbled (broken font decoding, mojibake).
   */
  COcrReason_SuspectedGarbledText = 0,
  /**
   * A scanned image page with no usable text layer.
   */
  COcrReason_Scanned = 1,
  /**
   * No extractable text and no image to OCR.
   */
  COcrReason_NoText = 2,
  /**
   * Text drawn as vector outlines rather than text operators.
   */
  COcrReason_VectorText = 3,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  COcrReason_ReservedMax = 2147483647,
} COcrReason;

/**
 * C-compatible error codes returned by the FFI functions.
 */
typedef enum {
  PdfInspectorError_Success = 0,
  PdfInspectorError_IoError = 1,
  PdfInspectorError_ParseError = 2,
  PdfInspectorError_Encrypted = 3,
  PdfInspectorError_InvalidStructure = 4,
  PdfInspectorError_NotAPdf = 5,
  PdfInspectorError_NullPointer = 6,
  PdfInspectorError_Panic = 7,
  PdfInspectorError_InvalidUtf8 = 8,
  PdfInspectorError_InvalidArgument = 9,
  /**
   * Not a real error code; forces this enum to 4-byte width under `-fshort-enums`.
   */
  PdfInspectorError_ReservedMax = 2147483647,
} PdfInspectorError;

/**
 * FFI-safe representation of PdfType.
 */
typedef enum {
  CPdfType_Unknown = -1,
  CPdfType_TextBased = 0,
  CPdfType_Scanned = 1,
  CPdfType_ImageBased = 2,
  CPdfType_Mixed = 3,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  CPdfType_ReservedMax = 2147483647,
} CPdfType;

/**
 * Processing modes accepted by `pdf_inspector_options_set_mode`.
 */
typedef enum {
  CProcessMode_DetectOnly = 0,
  CProcessMode_Analyze = 1,
  CProcessMode_Full = 2,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  CProcessMode_ReservedMax = 2147483647,
} CProcessMode;

/**
 * Markdown profiles accepted by `pdf_inspector_options_set_profile`.
 */
typedef enum {
  CMarkdownProfile_Fidelity = 0,
  CMarkdownProfile_Compact = 1,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  CMarkdownProfile_ReservedMax = 2147483647,
} CMarkdownProfile;

/**
 * Scan strategies accepted by `pdf_inspector_options_set_scan_strategy`.
 * `Sample`/`Pages` carry data that doesn't fit the discriminant, so the
 * setter takes it via separate `sample_size`/`pages` parameters instead.
 */
typedef enum {
  /**
   * Scan all pages, stop at the first non-text page.
   */
  CScanStrategy_EarlyExit = 0,
  /**
   * Scan all pages, no early exit.
   */
  CScanStrategy_Full = 1,
  /**
   * Sample up to `sample_size` evenly distributed pages.
   */
  CScanStrategy_Sample = 2,
  /**
   * Only scan the 1-indexed pages listed in `pages`/`pages_count`.
   */
  CScanStrategy_Pages = 3,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  CScanStrategy_ReservedMax = 2147483647,
} CScanStrategy;

/**
 * Type of an item returned by positioned-text extraction.
 */
typedef enum {
  CTextItemType_Unknown = -1,
  CTextItemType_Text = 0,
  CTextItemType_Image = 1,
  CTextItemType_Link = 2,
  CTextItemType_FormField = 3,
  /**
   * Not a real value; forces this enum to 4-byte width under `-fshort-enums`.
   */
  CTextItemType_ReservedMax = 2147483647,
} CTextItemType;

/**
 * Borrowed UTF-8 byte view supplied to or returned by the API. The bytes are
 * not NUL-terminated. Returned bytes must not be freed by the caller.
 */
typedef struct {
  const uint8_t *ptr;
  size_t len;
} CByteView;

/**
 * One caller-supplied PDF `re`-operator rectangle for table detection in
 * `pdf_inspector_to_markdown_from_items`. Coordinates are PDF points in the
 * same bottom-left-origin space as positioned-item coordinates. `page` is
 * 1-indexed.
 */
typedef struct {
  uint32_t page;
  float x;
  float y;
  float width;
  float height;
} CPdfRect;

/**
 * Rectangle supplied to region extraction, in PDF points with a top-left
 * origin. The two corners may be supplied in either order.
 */
typedef struct {
  float x1;
  float y1;
  float x2;
  float y2;
} CRegion;

/**
 * Regions to extract from one 1-indexed page. `regions` may be NULL only
 * when `regions_count` is zero.
 */
typedef struct {
  uint32_t page;
  const CRegion *regions;
  size_t regions_count;
} CPageRegions;

/**
 * One TSR cell rectangle or polygon in crop-image pixels. `coordinates`
 * must contain exactly 4 values (`x1,y1,x2,y2`) or 8 polygon coordinates.
 */
typedef struct {
  const float *coordinates;
  size_t coordinates_count;
} CTsrCellBBox;

/**
 * One table region plus externally supplied table-structure recognition
 * output. `page` is 1-indexed. All arrays are borrowed for the duration of
 * the extraction call and may be NULL only when their count is zero.
 */
typedef struct {
  uint32_t page;
  CRegion crop_pdf_pt_bbox;
  float render_dpi;
  const CByteView *structure_tokens;
  size_t structure_tokens_count;
  const CTsrCellBBox *cell_bboxes;
  size_t cell_bboxes_count;
} CTsrTableInput;

/**
 * Borrowed `u32` slice returned by array getters. The elements must not be
 * freed by the caller.
 */
typedef struct {
  const uint32_t *ptr;
  size_t len;
} CU32View;

/**
 * One caller-supplied positioned text item for
 * `pdf_inspector_text_items_result_add`. Coordinates are PDF points in the same
 * bottom-left-origin space the positioned-text getters return — not the
 * top-left origin region extraction uses. `page` is 1-indexed. `text`,
 * `font`, `font_tag`, and `link_url` are borrowed UTF-8 views, read only for
 * the duration of the call; a NULL view pointer is accepted only with zero
 * length and means empty. `item_type` takes a `CTextItemType` discriminant
 * (`Unknown` is rejected); `link_url` is observed only for
 * `CTextItemType_Link`. `flags` is a bitwise OR of
 * `PDF_INSPECTOR_TEXT_ITEM_FLAG_*` values.
 *
 * The numeric fields are exactly [`CTextItemMetrics`], which is what the
 * read side hands back, so an extracted item round-trips through this
 * descriptor without loss.
 */
typedef struct {
  uint32_t page;
  CByteView text;
  float x;
  float y;
  float width;
  float height;
  CByteView font;
  /**
   * Page-local font resource tag (`F2`, `C2_0`), as written in the
   * content stream. Empty for items with no originating PDF font
   * resource, which is every caller-built (e.g. OCR) item.
   */
  CByteView font_tag;
  float font_size;
  /**
   * A `CTextItemType` discriminant; plain `int32_t` because a `repr(C)`
   * enum field holding an out-of-range value is undefined behaviour.
   */
  int32_t item_type;
  CByteView link_url;
  uint32_t flags;
  int64_t mcid;
} CTextItemDescriptor;

/**
 * Every non-string field of an extracted positioned text item, copied out in
 * one call by `pdf_inspector_text_items_result_get_metrics`. The item's
 * `text`, `font`, `font_tag`, and `link_url` are borrowed views, so they stay
 * on their own getters rather than embedding a pointer whose lifetime rules
 * would differ from the rest of the struct — the same split
 * [`CTsrStructuredCell`] uses.
 *
 * `page` is 1-indexed, coordinates are PDF points with a bottom-left origin,
 * `item_type` is a `CTextItemType` discriminant, and `flags` is a bitwise OR
 * of `PDF_INSPECTOR_TEXT_ITEM_FLAG_*` values. `mcid` is meaningful only when
 * `flags` has `PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID` set — MCID 0 is a real
 * and common value, so the flag, not a zero, is what marks its absence.
 */
typedef struct {
  uint32_t page;
  float x;
  float y;
  float width;
  float height;
  float font_size;
  int32_t item_type;
  uint32_t flags;
  int64_t mcid;
} CTextItemMetrics;

/**
 * One detected vector-grid cell rectangle in crop-image pixels with a
 * top-left origin.
 */
typedef struct {
  float x1;
  float y1;
  float x2;
  float y2;
} CVectorGridCellBox;

/**
 * Fixed metadata for one resolved TSR cell. `page_pt_bbox` uses PDF points
 * with a top-left origin. Cell text is available through a separate getter.
 */
typedef struct {
  size_t row;
  size_t col;
  size_t rowspan;
  size_t colspan;
  bool is_header;
  CRegion page_pt_bbox;
} CTsrStructuredCell;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Return the C ABI major version. See the Versioning section in `docs/c-api.md`.
 */
uint32_t pdf_inspector_abi_version(void);

/**
 * Return the C ABI minor version. See the Versioning section in `docs/c-api.md`.
 */
uint32_t pdf_inspector_abi_minor(void);

/**
 * Map one OCR-reason string, as returned by any of the `_get_ocr_reason`
 * getters or `pdf_inspector_pages_result_get_entry_ocr_reason`, to a
 * `COcrReason` discriminant. Returns `COcrReason_Unknown` for a reason this
 * library does not define, which spares callers a table of string literals.
 */
COcrReason pdf_inspector_ocr_reason_from_string(CByteView reason);

/**
 * Estimate a PDF's page count by scanning the raw bytes, without parsing the
 * document. Orders of magnitude cheaper than opening the file and intended
 * for triage; it is an estimate, not an authoritative count. A NULL buffer is
 * accepted only when `size` is zero.
 */
PdfInspectorError pdf_inspector_estimate_page_count_from_bytes(const uint8_t *buffer,
                                                               size_t size,
                                                               uint32_t *count_out);

/**
 * Get the UTF-8 diagnostic message behind the most recent
 * `PdfInspectorError`-returning call on the calling thread. Returns `false`
 * and zeroes `out` if that call succeeded or left no diagnostic text. Getters
 * and `*_free` never touch this slot. The view stays valid until the next
 * fallible entry-point call on this thread.
 *
 * # Not safe from an M:N runtime
 *
 * The slot is keyed to the **OS thread**. Callers whose unit of work is not
 * an OS thread — Java virtual threads, Go goroutines without
 * `runtime.LockOSThread`, .NET `async` continuations — must use
 * [`pdf_inspector_last_error_copy`] instead. Another task sharing this OS
 * thread can overwrite the slot between the failing call and this one, and
 * because that frees the string, the returned view can dangle. See the
 * "Error diagnostics" section of `docs/c-api.md`.
 */
bool pdf_inspector_last_error_message(CByteView *out);

/**
 * Copy the most recent diagnostic on the calling thread into `buf`, and
 * write the error code that produced it to `code_out` (may be NULL).
 *
 * Returns the diagnostic's **full** length in bytes, as `snprintf` does, so a
 * return greater than `cap` means the copy was truncated and the return value
 * is the buffer size needed. Returns 0 when the last fallible call succeeded
 * or left no diagnostic text. The bytes are UTF-8 and not NUL-terminated;
 * `buf` may be NULL only when `cap` is zero, which is how you ask for the
 * length alone.
 *
 * # Prefer this over `pdf_inspector_last_error_message` off an M:N runtime
 *
 * [`pdf_inspector_last_error_message`] hands back a pointer into the
 * thread-local slot, which stays valid only until this OS thread's next
 * fallible call. When the caller's unit of work is not an OS thread — a Java
 * virtual thread, a goroutine, a .NET `async` continuation — another task can
 * share the same OS thread and free that string underneath the pointer.
 *
 * This entry point reads *and* copies inside a single call, so no other task
 * can interleave: the diagnostic either arrives intact or does not arrive.
 * That removes the dangling read, but not the possibility of reading a
 * *different* task's diagnostic. `code_out` is what discriminates: it always
 * carries the code the recorded call returned, whether or not that call left
 * any text, so `code_out` matching the code you just got back means the slot
 * is yours — a length of 0 then simply means your error carries no message.
 * A mismatch means another task overwrote it. `PdfInspectorError_Success`
 * appears only when the slot is genuinely empty.
 */
size_t pdf_inspector_last_error_copy(uint8_t *buf, size_t cap, int32_t *code_out);

/**
 * Create a new options handle with default settings, published through
 * `options_out`. Must be freed with `pdf_inspector_options_free`.
 *
 * Reports failure the same way every other allocating entry point does —
 * through `PdfInspectorError`, with the out-parameter zeroed first — rather
 * than through a NULL return, so a caller reading
 * `pdf_inspector_last_error_message` afterwards sees this call's diagnostic
 * and not a stale one.
 */
PdfInspectorError pdf_inspector_options_new(CPdfOptions **options_out);

/**
 * Free a `CPdfOptions` instance.
 */
void pdf_inspector_options_free(CPdfOptions *options);

/**
 * Set the processing mode to a `CProcessMode` value.
 * Out-of-range values are rejected with `InvalidArgument`.
 */
PdfInspectorError pdf_inspector_options_set_mode(CPdfOptions *options, int32_t mode);

/**
 * Set the password for decrypting an encrypted PDF.
 * Pass NULL to clear the password.
 */
PdfInspectorError pdf_inspector_options_set_password(CPdfOptions *options, const char *password);

/**
 * Limit processing to specific 1-indexed page. Can be called multiple times.
 * Page 0 has no 1-indexed meaning and is rejected with `InvalidArgument`.
 */
PdfInspectorError pdf_inspector_options_add_page(CPdfOptions *options, uint32_t page);

/**
 * Clear the page filter, restoring processing of every page.
 */
PdfInspectorError pdf_inspector_options_clear_pages(CPdfOptions *options);

/**
 * Set whether to detect headers by font size.
 */
PdfInspectorError pdf_inspector_options_set_detect_headers(CPdfOptions *options, bool enable);

/**
 * Set whether to detect list items.
 */
PdfInspectorError pdf_inspector_options_set_detect_lists(CPdfOptions *options, bool enable);

/**
 * Set whether to detect code blocks.
 */
PdfInspectorError pdf_inspector_options_set_detect_code(CPdfOptions *options, bool enable);

/**
 * Set whether to remove standalone page numbers.
 */
PdfInspectorError pdf_inspector_options_set_remove_page_numbers(CPdfOptions *options, bool enable);

/**
 * Set whether to convert URLs to markdown links.
 */
PdfInspectorError pdf_inspector_options_set_format_urls(CPdfOptions *options, bool enable);

/**
 * Set whether to fix hyphenation (broken words across lines).
 */
PdfInspectorError pdf_inspector_options_set_fix_hyphenation(CPdfOptions *options, bool enable);

/**
 * Set whether to detect and format bold text.
 */
PdfInspectorError pdf_inspector_options_set_detect_bold(CPdfOptions *options, bool enable);

/**
 * Set whether to detect and format italic text.
 */
PdfInspectorError pdf_inspector_options_set_detect_italic(CPdfOptions *options, bool enable);

/**
 * Set whether to emit `<u>` runs for text with an underline.
 */
PdfInspectorError pdf_inspector_options_set_detect_underline(CPdfOptions *options, bool enable);

/**
 * Set whether to include image placeholders in output.
 */
PdfInspectorError pdf_inspector_options_set_include_images(CPdfOptions *options, bool enable);

/**
 * Set whether to include extracted hyperlinks.
 */
PdfInspectorError pdf_inspector_options_set_include_links(CPdfOptions *options, bool enable);

/**
 * Set whether to insert page break markers (<!-- Page N -->) between pages.
 */
PdfInspectorError pdf_inspector_options_set_include_page_numbers(CPdfOptions *options, bool enable);

/**
 * Set whether to strip repeated headers/footers.
 */
PdfInspectorError pdf_inspector_options_set_strip_headers_footers(CPdfOptions *options,
                                                                  bool enable);

/**
 * Set the markdown profile to a `CMarkdownProfile` value.
 * Out-of-range values are rejected with `InvalidArgument`.
 */
PdfInspectorError pdf_inspector_options_set_profile(CPdfOptions *options, int32_t profile);

/**
 * Set minimum text operator count per page to consider as text-based.
 */
PdfInspectorError pdf_inspector_options_set_min_text_ops_per_page(CPdfOptions *options,
                                                                  uint32_t count);

/**
 * Set threshold ratio of text pages to total pages for classification.
 * Only finite values in the inclusive range `0.0..=1.0` are accepted.
 */
PdfInspectorError pdf_inspector_options_set_text_page_ratio_threshold(CPdfOptions *options,
                                                                      float threshold);

/**
 * Set the page-detection scan strategy from a `CScanStrategy` discriminant.
 * `sample_size` is used only for `CScanStrategy_Sample` (scan up to this
 * many evenly distributed pages; 0 is rejected). `pages`/`pages_count` are
 * used only for `CScanStrategy_Pages` (1-indexed pages to scan; NULL with a
 * nonzero count, an empty list, or any page number of 0 is rejected). Both
 * are ignored for `CScanStrategy_EarlyExit` and `CScanStrategy_Full`.
 * Out-of-range `strategy` values are rejected with `InvalidArgument`.
 */
PdfInspectorError pdf_inspector_options_set_scan_strategy(CPdfOptions *options,
                                                          int32_t strategy,
                                                          uint32_t sample_size,
                                                          const uint32_t *pages,
                                                          size_t pages_count);

/**
 * Set the base font size (in points) used as the body-text baseline for
 * header-size comparisons. A finite value `>= 1.0` sets an explicit
 * override; any other finite value (`< 1.0`, including 0 and negatives)
 * clears the override and restores automatic detection from the document's
 * dominant font size, which is also the default. NaN and infinite values
 * are rejected with `InvalidArgument`.
 */
PdfInspectorError pdf_inspector_options_set_base_font_size(CPdfOptions *options, float size);

/**
 * Process a PDF file with options.
 * Returns Success on success and populates `result_out` with an opaque `CPdfProcessResult` pointer.
 * If `options` is NULL, default options are used.
 * The output result must be freed using `pdf_inspector_process_result_free`.
 */
PdfInspectorError pdf_inspector_process_pdf(const char *path,
                                            const CPdfOptions *options,
                                            CPdfProcessResult **result_out);

/**
 * Process PDF bytes with options. A NULL buffer is accepted only when `size` is zero.
 * Returns Success on success and populates `result_out` with an opaque `CPdfProcessResult` pointer.
 * If `options` is NULL, default options are used.
 * The output result must be freed using `pdf_inspector_process_result_free`.
 */
PdfInspectorError pdf_inspector_process_pdf_mem(const uint8_t *buffer,
                                                size_t size,
                                                const CPdfOptions *options,
                                                CPdfProcessResult **result_out);

/**
 * Run full PDF type detection on a file. If `options` is NULL, defaults are
 * used. Only the detection settings and password are observed; processing,
 * Markdown, and page-filter settings are ignored. Free the returned handle
 * with `pdf_inspector_pdf_type_result_free`.
 */
PdfInspectorError pdf_inspector_detect_pdf_type(const char *path,
                                                const CPdfOptions *options,
                                                CPdfTypeResult **result_out);

/**
 * Run full PDF type detection on PDF bytes. A NULL buffer is accepted only
 * when `size` is zero. If `options` is NULL, defaults are used. Only the
 * detection settings and password are observed; processing, Markdown, and
 * page-filter settings are ignored. Free the returned handle with
 * `pdf_inspector_pdf_type_result_free`.
 */
PdfInspectorError pdf_inspector_detect_pdf_type_mem(const uint8_t *buffer,
                                                    size_t size,
                                                    const CPdfOptions *options,
                                                    CPdfTypeResult **result_out);

/**
 * Classify a PDF file without extracting text. `password` decrypts an
 * encrypted PDF (NULL = none; see `pdf_inspector_options_set_password`).
 * Returns Success on success and populates `result_out` with an opaque
 * `CPdfClassification` pointer.
 * Must be freed with `pdf_inspector_classification_free`.
 */
PdfInspectorError pdf_inspector_classify_pdf(const char *path,
                                             const char *password,
                                             CPdfClassification **result_out);

/**
 * Classify a PDF from a memory buffer without extracting text.
 * A NULL buffer is accepted only when `size` is zero. `password` decrypts an
 * encrypted PDF (NULL = none; see `pdf_inspector_options_set_password`).
 * Returns Success on success and populates `result_out` with an opaque `CPdfClassification` pointer.
 * Must be freed with `pdf_inspector_classification_free`.
 */
PdfInspectorError pdf_inspector_classify_pdf_mem(const uint8_t *buffer,
                                                 size_t size,
                                                 const char *password,
                                                 CPdfClassification **result_out);

/**
 * Convert UTF-8 plain text to basic Markdown. A NULL `text` pointer is
 * accepted only when `size` is zero. If `options` is NULL, defaults are used.
 * Only Markdown settings are observed; processing mode, detection settings,
 * password, and page filters are ignored. The result must be freed with
 * `pdf_inspector_text_result_free`.
 */
PdfInspectorError pdf_inspector_to_markdown(const uint8_t *text,
                                            size_t size,
                                            const CPdfOptions *options,
                                            CTextResult **result_out);

/**
 * Convert positioned text items to Markdown. `items` is borrowed and remains
 * valid and reusable after the call; it may come from
 * `pdf_inspector_extract_text_with_positions` or be caller-built with
 * `pdf_inspector_text_items_result_new`/`_add`.
 *
 * `rects`/`rects_count` optionally supply PDF `re`-operator rectangles for
 * rectangle-based table detection; pass NULL/0 to convert without it. The
 * array is borrowed only for the duration of the call.
 * `document_page_count` is the owning PDF's authoritative page count, so
 * trailing blank or unextracted pages count toward document-wide header,
 * footer, and folio coverage; pass 0 (it is a count, not a page number) to
 * fall back to the highest item page.
 *
 * If `options` is NULL, defaults are used. Only Markdown settings are
 * observed; processing mode, detection settings, password, and page filters
 * are ignored. No path-line geometry or structure-tree context is available
 * on this path. The result must be freed with
 * `pdf_inspector_text_result_free`.
 */
PdfInspectorError pdf_inspector_to_markdown_from_items(const CTextItemsResult *items,
                                                       const CPdfRect *rects,
                                                       size_t rects_count,
                                                       uint32_t document_page_count,
                                                       const CPdfOptions *options,
                                                       CTextResult **result_out);

/**
 * Extract plain text from a PDF file. `password` decrypts an encrypted PDF
 * (NULL = none; see `pdf_inspector_options_set_password`).
 * Populates `result_out` with an opaque `CTextResult` pointer; read the bytes
 * with `pdf_inspector_text_result_get_text`.
 * Must be freed with `pdf_inspector_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text(const char *path,
                                             const char *password,
                                             CTextResult **result_out);

/**
 * Extract plain text from PDF bytes. A NULL buffer is accepted only when `size` is zero.
 * `password` decrypts an encrypted PDF (NULL = none; see `pdf_inspector_options_set_password`).
 * Populates `result_out` with an opaque `CTextResult` pointer; read the bytes
 * with `pdf_inspector_text_result_get_text`.
 * Must be freed with `pdf_inspector_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text_mem(const uint8_t *buffer,
                                                 size_t size,
                                                 const char *password,
                                                 CTextResult **result_out);

/**
 * Extract positioned text items from a PDF file.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
 * `password` decrypts an encrypted PDF (NULL = none). Must be freed with
 * `pdf_inspector_text_items_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text_with_positions(const char *path,
                                                            const uint32_t *pages,
                                                            size_t pages_count,
                                                            const char *password,
                                                            CTextItemsResult **result_out);

/**
 * Extract positioned text items from PDF bytes.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid. A NULL `buffer` is
 * accepted only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
 * The result must be freed with `pdf_inspector_text_items_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text_with_positions_mem(const uint8_t *buffer,
                                                                size_t size,
                                                                const uint32_t *pages,
                                                                size_t pages_count,
                                                                const char *password,
                                                                CTextItemsResult **result_out);

/**
 * Extract tagged-PDF structure-tree elements from a PDF file.
 * Returns an empty result for untagged PDFs. Entries are sorted by `(page, mcid)`; join those
 * fields against positioned text items to attach resolved standard or RoleMap roles to text.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
 * `password` decrypts an encrypted PDF (NULL = none). Must be freed with
 * `pdf_inspector_structure_elements_result_free`.
 */
PdfInspectorError pdf_inspector_extract_structure_elements(const char *path,
                                                           const uint32_t *pages,
                                                           size_t pages_count,
                                                           const char *password,
                                                           CStructureElementsResult **result_out);

/**
 * Extract tagged-PDF structure-tree elements from PDF bytes.
 * Returns an empty result for untagged PDFs. Entries are sorted by `(page, mcid)`; join those
 * fields against positioned text items to attach resolved standard or RoleMap roles to text.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid. A NULL `buffer` is
 * accepted only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
 * The result must be freed with `pdf_inspector_structure_elements_result_free`.
 */
PdfInspectorError pdf_inspector_extract_structure_elements_mem(const uint8_t *buffer,
                                                               size_t size,
                                                               const uint32_t *pages,
                                                               size_t pages_count,
                                                               const char *password,
                                                               CStructureElementsResult **result_out);

/**
 * Extract pages markdown and metadata from a PDF file.
 * Populates `result_out` with `CPagesExtractionResult`.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
 * `password` decrypts an encrypted PDF (NULL = none).
 * Must be freed with `pdf_inspector_pages_result_free`.
 */
PdfInspectorError pdf_inspector_extract_pages_markdown(const char *path,
                                                       const uint32_t *pages,
                                                       size_t pages_count,
                                                       const char *password,
                                                       CPagesExtractionResult **result_out);

/**
 * Extract pages markdown and metadata from PDF bytes.
 * Populates `result_out` with `CPagesExtractionResult`.
 * `pages` contains 1-indexed page numbers. `pages == NULL && pages_count == 0` extracts all pages.
 * A NULL `pages` with a nonzero count, or a page number of 0, is invalid.
 * A NULL `buffer` is accepted only when `size` is zero. `password` decrypts
 * an encrypted PDF (NULL = none).
 * Must be freed with `pdf_inspector_pages_result_free`.
 */
PdfInspectorError pdf_inspector_extract_pages_markdown_mem(const uint8_t *buffer,
                                                           size_t size,
                                                           const uint32_t *pages,
                                                           size_t pages_count,
                                                           const char *password,
                                                           CPagesExtractionResult **result_out);

/**
 * Extract text within bounding-box regions from a PDF file. Page numbers in
 * `page_regions` are 1-indexed; coordinates are PDF points with a top-left
 * origin. Results are parallel to the input pages and regions. `password`
 * decrypts an encrypted PDF (NULL = none). The result must be freed with
 * `pdf_inspector_region_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text_in_regions(const char *path,
                                                        const CPageRegions *page_regions,
                                                        size_t page_regions_count,
                                                        const char *password,
                                                        CRegionTextResult **result_out);

/**
 * Extract text within bounding-box regions from PDF bytes. Page numbers in
 * `page_regions` are 1-indexed; coordinates are PDF points with a top-left
 * origin. A NULL `buffer` is accepted only when `size` is zero. `password`
 * decrypts an encrypted PDF (NULL = none). The result must be freed with
 * `pdf_inspector_region_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_text_in_regions_mem(const uint8_t *buffer,
                                                            size_t size,
                                                            const CPageRegions *page_regions,
                                                            size_t page_regions_count,
                                                            const char *password,
                                                            CRegionTextResult **result_out);

/**
 * Extract markdown tables within bounding-box regions from a PDF file. Page
 * numbers in `page_regions` are 1-indexed; coordinates are PDF points with a
 * top-left origin. A region with no reliable table has empty text and
 * `needs_ocr` set. `password` decrypts an encrypted PDF (NULL = none). The
 * result must be freed with `pdf_inspector_region_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_in_regions(const char *path,
                                                          const CPageRegions *page_regions,
                                                          size_t page_regions_count,
                                                          const char *password,
                                                          CRegionTextResult **result_out);

/**
 * Extract markdown tables within bounding-box regions from PDF bytes. Page
 * numbers in `page_regions` are 1-indexed; coordinates are PDF points with a
 * top-left origin. A NULL `buffer` is accepted only when `size` is zero. A
 * region with no reliable table has empty text and `needs_ocr` set. `password`
 * decrypts an encrypted PDF (NULL = none). The result must be freed with
 * `pdf_inspector_region_text_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_in_regions_mem(const uint8_t *buffer,
                                                              size_t size,
                                                              const CPageRegions *page_regions,
                                                              size_t page_regions_count,
                                                              const char *password,
                                                              CRegionTextResult **result_out);

/**
 * Detect a vector ruled-line or rectangle grid inside one region of a PDF
 * file. `page` is 1-indexed. `region` uses PDF points with a top-left origin;
 * its corners may be supplied in either order. `render_dpi` must be finite and
 * positive, and the scaled crop dimensions must remain finite. It controls
 * the crop-pixel coordinates returned for cells.
 * `password` decrypts an encrypted PDF (NULL = none). Success always returns a
 * handle, including when no grid is detected. Free it with
 * `pdf_inspector_vector_grid_result_free`.
 */
PdfInspectorError pdf_inspector_detect_vector_grid_in_region(const char *path,
                                                             uint32_t page,
                                                             const CRegion *region,
                                                             float render_dpi,
                                                             const char *password,
                                                             CVectorGridResult **result_out);

/**
 * Detect a vector ruled-line or rectangle grid inside one region of PDF
 * bytes. `page` is 1-indexed. `region` uses PDF points with a top-left origin;
 * its corners may be supplied in either order. `render_dpi` must be finite and
 * positive, and the scaled crop dimensions must remain finite. It controls
 * the crop-pixel coordinates returned for cells. A NULL `buffer` is accepted
 * only when `size` is zero. `password` decrypts an
 * encrypted PDF (NULL = none). Success always returns a handle, including when
 * no grid is detected. Free it with `pdf_inspector_vector_grid_result_free`.
 */
PdfInspectorError pdf_inspector_detect_vector_grid_in_region_mem(const uint8_t *buffer,
                                                                 size_t size,
                                                                 uint32_t page,
                                                                 const CRegion *region,
                                                                 float render_dpi,
                                                                 const char *password,
                                                                 CVectorGridResult **result_out);

/**
 * Extract production-ready markdown tables using externally supplied table
 * structure recognition output. Pages are 1-indexed; crop coordinates are
 * PDF points with a top-left origin and ordered corners. Cell coordinates are
 * crop-image pixels and may contain 4-value rectangles or 8-value polygons.
 * Token and cell counts must match the parsed structure; row spans must fit
 * the declared rows and column spans are limited to 25. `password` decrypts an
 * encrypted PDF (NULL = none). Input arrays are borrowed only for this call.
 * Free the result with `pdf_inspector_tsr_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure_auto(const char *path,
                                                                   const CTsrTableInput *inputs,
                                                                   size_t inputs_count,
                                                                   const char *password,
                                                                   CTsrTableExtractionResult **result_out);

/**
 * Extract production-ready markdown tables using externally supplied table
 * structure recognition output and PDF bytes. Pages are 1-indexed; crop
 * coordinates are PDF points with a top-left origin and ordered corners. Cell
 * coordinates are crop-image pixels and may contain 4-value rectangles or
 * 8-value polygons. Token and cell counts must match the parsed structure;
 * row spans must fit the declared rows and column spans are limited to 25. A
 * NULL `buffer` is accepted only when `size` is zero. `password` decrypts an
 * encrypted PDF (NULL = none). Input arrays are borrowed only for this call.
 * Free the result with `pdf_inspector_tsr_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure_auto_mem(const uint8_t *buffer,
                                                                       size_t size,
                                                                       const CTsrTableInput *inputs,
                                                                       size_t inputs_count,
                                                                       const char *password,
                                                                       CTsrTableExtractionResult **result_out);

/**
 * Extract raw markdown tables using externally supplied table-structure
 * recognition output, from a PDF file. Identical inputs to
 * `pdf_inspector_extract_tables_with_structure_auto`, but the structure is
 * rendered as given: no quality repair and no heuristic fallback, so a
 * pathological token stream produces a pathological table. Use it to compare
 * the two paths (eval harnesses); prefer the auto path in production. Each
 * result's fallback reason is always absent. Free the result with
 * `pdf_inspector_tsr_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure(const char *path,
                                                              const CTsrTableInput *inputs,
                                                              size_t inputs_count,
                                                              const char *password,
                                                              CTsrTableExtractionResult **result_out);

/**
 * Extract raw markdown tables using externally supplied table-structure
 * recognition output, from PDF bytes. See
 * `pdf_inspector_extract_tables_with_structure` for the semantics; a NULL
 * `buffer` is accepted only when `size` is zero. Free the result with
 * `pdf_inspector_tsr_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure_mem(const uint8_t *buffer,
                                                                  size_t size,
                                                                  const CTsrTableInput *inputs,
                                                                  size_t inputs_count,
                                                                  const char *password,
                                                                  CTsrTableExtractionResult **result_out);

/**
 * Resolve raw structured cells from externally supplied table-structure
 * recognition output. Pages are 1-indexed. Input geometry, token grammar,
 * span limits, and borrowing rules are the same as for
 * `pdf_inspector_extract_tables_with_structure_auto`. This path does not run
 * auto quality repair or heuristic fallback. `password` decrypts an encrypted
 * PDF (NULL = none). Free the result with `pdf_inspector_tsr_cells_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure_cells(const char *path,
                                                                    const CTsrTableInput *inputs,
                                                                    size_t inputs_count,
                                                                    const char *password,
                                                                    CTsrStructuredCellsResult **result_out);

/**
 * Resolve raw structured cells from externally supplied table-structure
 * recognition output and PDF bytes. Pages are 1-indexed. Input geometry,
 * token grammar, span limits, and borrowing rules are the same as for
 * `pdf_inspector_extract_tables_with_structure_auto_mem`. This path does not
 * run auto quality repair or heuristic fallback. A NULL `buffer` is accepted
 * only when `size` is zero. `password` decrypts an encrypted PDF (NULL = none).
 * Free the result with `pdf_inspector_tsr_cells_result_free`.
 */
PdfInspectorError pdf_inspector_extract_tables_with_structure_cells_mem(const uint8_t *buffer,
                                                                        size_t size,
                                                                        const CTsrTableInput *inputs,
                                                                        size_t inputs_count,
                                                                        const char *password,
                                                                        CTsrStructuredCellsResult **result_out);

/**
 * Free a `CTextResult` instance.
 */
void pdf_inspector_text_result_free(CTextResult *result);

/**
 * Get the extracted UTF-8 text bytes. Extracted text may legitimately contain
 * NUL bytes. Returns `false` and zeroes `out` for a NULL result or output.
 * The view remains valid until `result` is freed.
 */
bool pdf_inspector_text_result_get_text(const CTextResult *result, CByteView *out);

/**
 * Free a `CPdfProcessResult` instance.
 */
void pdf_inspector_process_result_free(CPdfProcessResult *result);

/**
 * Get the detected PDF type.
 */
CPdfType pdf_inspector_process_result_get_type(const CPdfProcessResult *result);

/**
 * Get the total page count.
 */
uint32_t pdf_inspector_process_result_get_page_count(const CPdfProcessResult *result);

/**
 * Get the processing time in milliseconds.
 */
uint64_t pdf_inspector_process_result_get_processing_time_ms(const CPdfProcessResult *result);

/**
 * Get the confidence score (0.0 - 1.0).
 */
float pdf_inspector_process_result_get_confidence(const CPdfProcessResult *result);

/**
 * Returns true if encoding issues were detected.
 */
bool pdf_inspector_process_result_has_encoding_issues(const CPdfProcessResult *result);

/**
 * Returns true if complex layout (tables or columns) was detected.
 */
bool pdf_inspector_process_result_is_complex_layout(const CPdfProcessResult *result);

/**
 * Free a `CPdfClassification` instance.
 */
void pdf_inspector_classification_free(CPdfClassification *classification);

/**
 * Get the detected PDF type from classification.
 */
CPdfType pdf_inspector_classification_get_type(const CPdfClassification *classification);

/**
 * Get total page count from classification.
 */
uint32_t pdf_inspector_classification_get_page_count(const CPdfClassification *classification);

/**
 * Get confidence from classification.
 */
float pdf_inspector_classification_get_confidence(const CPdfClassification *classification);

/**
 * Free a `CPdfTypeResult` instance.
 */
void pdf_inspector_pdf_type_result_free(CPdfTypeResult *result);

/**
 * Get the detected PDF type.
 */
CPdfType pdf_inspector_pdf_type_result_get_type(const CPdfTypeResult *result);

/**
 * Get the total number of pages in the document.
 */
uint32_t pdf_inspector_pdf_type_result_get_page_count(const CPdfTypeResult *result);

/**
 * Get the number of pages sampled during detection.
 */
uint32_t pdf_inspector_pdf_type_result_get_pages_sampled(const CPdfTypeResult *result);

/**
 * Get the number of sampled pages classified as having text.
 */
uint32_t pdf_inspector_pdf_type_result_get_pages_with_text(const CPdfTypeResult *result);

/**
 * Get the confidence score (0.0 - 1.0).
 */
float pdf_inspector_pdf_type_result_get_confidence(const CPdfTypeResult *result);

/**
 * Get the optional document-title UTF-8 bytes. Returns `false` and zeroes
 * `out` when the title is absent or either pointer is NULL.
 */
bool pdf_inspector_pdf_type_result_get_title(const CPdfTypeResult *result, CByteView *out);

/**
 * Return whether OCR is recommended for better extraction.
 */
bool pdf_inspector_pdf_type_result_is_ocr_recommended(const CPdfTypeResult *result);

/**
 * Get the borrowed array of 1-indexed page numbers needing OCR.
 */
bool pdf_inspector_pdf_type_result_get_pages_needing_ocr(const CPdfTypeResult *result,
                                                         CU32View *out);

/**
 * Get the number of per-page OCR-reason entries on a `CPdfTypeResult`. Returns
 * zero for a NULL handle.
 */
size_t pdf_inspector_pdf_type_result_get_ocr_page_count(const CPdfTypeResult *result);

/**
 * Get the 1-indexed page number for one OCR-reason entry on a `CPdfTypeResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
uint32_t pdf_inspector_pdf_type_result_get_ocr_page_number(const CPdfTypeResult *result,
                                                           size_t index);

/**
 * Get the number of reason strings in one OCR-reason entry on a `CPdfTypeResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
size_t pdf_inspector_pdf_type_result_get_ocr_page_reason_count(const CPdfTypeResult *result,
                                                               size_t index);

/**
 * Get one OCR reason's UTF-8 bytes from a `CPdfTypeResult`. Returns `false` and
 * zeroes `out` when the requested reason is absent or an input pointer is
 * NULL. The view remains valid until `result` is freed.
 */
bool pdf_inspector_pdf_type_result_get_ocr_page_reason(const CPdfTypeResult *result,
                                                       size_t index,
                                                       size_t reason_index,
                                                       CByteView *out);

/**
 * Free a `CPagesExtractionResult` instance.
 */
void pdf_inspector_pages_result_free(CPagesExtractionResult *result);

/**
 * Get number of extracted pages.
 */
size_t pdf_inspector_pages_result_get_entry_count(const CPagesExtractionResult *result);

/**
 * Get the 1-indexed page number of the page at `index`, matching the base used
 * by every other page number in this ABI. Returns 0 for an out-of-range index.
 */
uint32_t pdf_inspector_pages_result_get_entry_page_number(const CPagesExtractionResult *result,
                                                          size_t index);

/**
 * Get whether page at index needs OCR.
 */
bool pdf_inspector_pages_result_get_entry_needs_ocr(const CPagesExtractionResult *result,
                                                    size_t index);

/**
 * Get whether any page has tables or columns.
 */
bool pdf_inspector_pages_result_is_complex(const CPagesExtractionResult *result);

/**
 * Get the Markdown UTF-8 bytes. Returns `false` and zeroes `out` when
 * Markdown is absent or either pointer is NULL. The view remains valid until
 * `result` is freed.
 */
bool pdf_inspector_process_result_get_markdown(const CPdfProcessResult *result, CByteView *out);

/**
 * Get the title UTF-8 bytes. Returns `false` and zeroes `out` when the title
 * is absent or either pointer is NULL. The view remains valid until `result`
 * is freed.
 */
bool pdf_inspector_process_result_get_title(const CPdfProcessResult *result, CByteView *out);

/**
 * Get the borrowed array of 1-indexed page numbers needing OCR.
 */
bool pdf_inspector_process_result_get_pages_needing_ocr(const CPdfProcessResult *result,
                                                        CU32View *out);

/**
 * Get the borrowed array of 1-indexed page numbers with tables.
 */
bool pdf_inspector_process_result_get_pages_with_tables(const CPdfProcessResult *result,
                                                        CU32View *out);

/**
 * Get the borrowed array of 1-indexed page numbers with columns.
 */
bool pdf_inspector_process_result_get_pages_with_columns(const CPdfProcessResult *result,
                                                         CU32View *out);

/**
 * Get the number of per-page OCR-reason entries on a `CPdfProcessResult`. Returns
 * zero for a NULL handle.
 */
size_t pdf_inspector_process_result_get_ocr_page_count(const CPdfProcessResult *result);

/**
 * Get the 1-indexed page number for one OCR-reason entry on a `CPdfProcessResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
uint32_t pdf_inspector_process_result_get_ocr_page_number(const CPdfProcessResult *result,
                                                          size_t index);

/**
 * Get the number of reason strings in one OCR-reason entry on a `CPdfProcessResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
size_t pdf_inspector_process_result_get_ocr_page_reason_count(const CPdfProcessResult *result,
                                                              size_t index);

/**
 * Get one OCR reason's UTF-8 bytes from a `CPdfProcessResult`. Returns `false` and
 * zeroes `out` when the requested reason is absent or an input pointer is
 * NULL. The view remains valid until `result` is freed.
 */
bool pdf_inspector_process_result_get_ocr_page_reason(const CPdfProcessResult *result,
                                                      size_t index,
                                                      size_t reason_index,
                                                      CByteView *out);

/**
 * Get the page Markdown UTF-8 bytes at `index`. Returns `false` and zeroes
 * `out` for an invalid index or input pointer.
 */
bool pdf_inspector_pages_result_get_entry_markdown(const CPagesExtractionResult *result,
                                                   size_t index,
                                                   CByteView *out);

/**
 * Get the page OCR reason UTF-8 bytes at `index`. Returns `false` and zeroes
 * `out` when the reason is absent or an input pointer is invalid.
 */
bool pdf_inspector_pages_result_get_entry_ocr_reason(const CPagesExtractionResult *result,
                                                     size_t index,
                                                     CByteView *out);

/**
 * Get the borrowed array of 1-indexed page numbers needing OCR.
 */
bool pdf_inspector_pages_result_get_pages_needing_ocr(const CPagesExtractionResult *result,
                                                      CU32View *out);

/**
 * Get the borrowed array of 1-indexed page numbers with tables.
 */
bool pdf_inspector_pages_result_get_pages_with_tables(const CPagesExtractionResult *result,
                                                      CU32View *out);

/**
 * Get the borrowed array of 1-indexed page numbers with columns.
 */
bool pdf_inspector_pages_result_get_pages_with_columns(const CPagesExtractionResult *result,
                                                       CU32View *out);

/**
 * Get the number of per-page OCR-reason entries on a `CPagesExtractionResult`. Returns
 * zero for a NULL handle.
 */
size_t pdf_inspector_pages_result_get_ocr_page_count(const CPagesExtractionResult *result);

/**
 * Get the 1-indexed page number for one OCR-reason entry on a `CPagesExtractionResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
uint32_t pdf_inspector_pages_result_get_ocr_page_number(const CPagesExtractionResult *result,
                                                        size_t index);

/**
 * Get the number of reason strings in one OCR-reason entry on a `CPagesExtractionResult`.
 * Returns zero for a NULL handle or an out-of-range index.
 */
size_t pdf_inspector_pages_result_get_ocr_page_reason_count(const CPagesExtractionResult *result,
                                                            size_t index);

/**
 * Get one OCR reason's UTF-8 bytes from a `CPagesExtractionResult`. Returns `false` and
 * zeroes `out` when the requested reason is absent or an input pointer is
 * NULL. The view remains valid until `result` is freed.
 */
bool pdf_inspector_pages_result_get_ocr_page_reason(const CPagesExtractionResult *result,
                                                    size_t index,
                                                    size_t reason_index,
                                                    CByteView *out);

/**
 * Get the borrowed array of page numbers needing OCR, 1-indexed like every
 * other page number in this ABI.
 */
bool pdf_inspector_classification_get_pages_needing_ocr(const CPdfClassification *classification,
                                                        CU32View *out);

/**
 * Create an empty caller-owned `CTextItemsResult`. Populate it with
 * `pdf_inspector_text_items_result_add`; it is then accepted everywhere an
 * extracted `CTextItemsResult` is (getters,
 * `pdf_inspector_to_markdown_from_items`). This is the entry point for
 * feeding externally produced positioned text — e.g. OCR output for regions
 * reported as `needs_ocr` — back through the Markdown converter.
 * Must be freed with `pdf_inspector_text_items_result_free`.
 */
PdfInspectorError pdf_inspector_text_items_result_new(CTextItemsResult **result_out);

/**
 * Append `descriptors_count` caller-supplied items to a `CTextItemsResult`,
 * copying every string, so the descriptor array is borrowed only for the
 * duration of the call. `descriptors` may be NULL only when
 * `descriptors_count` is zero. The call is atomic: on any error nothing is
 * appended. Items may be added in any order across any pages; the converter
 * sorts by position. Must not race with another use of the same handle.
 */
PdfInspectorError pdf_inspector_text_items_result_add(CTextItemsResult *items,
                                                      const CTextItemDescriptor *descriptors,
                                                      size_t descriptors_count);

/**
 * Free a `CTextItemsResult` instance.
 */
void pdf_inspector_text_items_result_free(CTextItemsResult *result);

/**
 * Get the number of positioned text items.
 */
size_t pdf_inspector_text_items_result_get_count(const CTextItemsResult *result);

/**
 * Copy an item's numeric and flag fields into `out`. Returns `false` and
 * zeroes `out` for an invalid item index or a NULL pointer. This is the read
 * counterpart of `CTextItemDescriptor`'s non-string fields; the item's text,
 * font, font tag, and link URL have their own borrowed-view getters.
 */
bool pdf_inspector_text_items_result_get_metrics(const CTextItemsResult *result,
                                                 size_t index,
                                                 CTextItemMetrics *out);

/**
 * Get an item's UTF-8 text bytes. Returns `false` and zeroes `out` for an
 * invalid item index or input pointer. The view remains valid until `result`
 * is freed.
 */
bool pdf_inspector_text_items_result_get_text(const CTextItemsResult *result,
                                              size_t index,
                                              CByteView *out);

/**
 * Get an item's font-name UTF-8 bytes. Returns `false` and zeroes `out` for an
 * invalid item index or input pointer. The view remains valid until `result`
 * is freed.
 */
bool pdf_inspector_text_items_result_get_font(const CTextItemsResult *result,
                                              size_t index,
                                              CByteView *out);

/**
 * Get an item's page-local font resource tag (`F2`, `C2_0`) as UTF-8 bytes —
 * the name the content stream selected the font by, as opposed to the
 * `/BaseFont` family `pdf_inspector_text_items_result_get_font` returns.
 * Present but empty for items with no originating PDF font resource. Returns
 * `false` and zeroes `out` for an invalid item index or input pointer. The
 * view remains valid until `result` is freed.
 */
bool pdf_inspector_text_items_result_get_font_tag(const CTextItemsResult *result,
                                                  size_t index,
                                                  CByteView *out);

/**
 * Get a link item's URL UTF-8 bytes. Returns `false` and zeroes `out` for a
 * non-link item, invalid index, or input pointer. The view remains valid until
 * `result` is freed.
 */
bool pdf_inspector_text_items_result_get_link_url(const CTextItemsResult *result,
                                                  size_t index,
                                                  CByteView *out);

/**
 * Free a `CStructureElementsResult` instance.
 */
void pdf_inspector_structure_elements_result_free(CStructureElementsResult *result);

/**
 * Get the number of tagged-PDF structure elements.
 */
size_t pdf_inspector_structure_elements_result_get_count(const CStructureElementsResult *result);

/**
 * Get a structure element's 1-indexed page number.
 */
uint32_t pdf_inspector_structure_elements_result_get_page(const CStructureElementsResult *result,
                                                          size_t index);

/**
 * Copy a structure element's marked-content ID into `out`. Returns `false`
 * and zeroes `out` for an invalid element index or a NULL pointer.
 *
 * This reports absence through the return value rather than a sentinel:
 * MCID 0 is the first marked-content ID on every page, so a `0` return
 * could not be told apart from a valid element.
 */
bool pdf_inspector_structure_elements_result_get_mcid(const CStructureElementsResult *result,
                                                      size_t index,
                                                      int64_t *out);

/**
 * Get a structure element's role UTF-8 bytes. Returns `false` and zeroes `out`
 * for an invalid element index or input pointer. The view remains valid until
 * `result` is freed.
 */
bool pdf_inspector_structure_elements_result_get_role(const CStructureElementsResult *result,
                                                      size_t index,
                                                      CByteView *out);

/**
 * Free a `CRegionTextResult` instance.
 */
void pdf_inspector_region_text_result_free(CRegionTextResult *result);

/**
 * Get the number of page entries in a region-text result.
 */
size_t pdf_inspector_region_text_result_get_entry_count(const CRegionTextResult *result);

/**
 * Get a page entry's 1-indexed page number.
 */
uint32_t pdf_inspector_region_text_result_get_entry_page_number(const CRegionTextResult *result,
                                                                size_t page_index);

/**
 * Get the number of region entries for one page entry.
 */
size_t pdf_inspector_region_text_result_get_region_count(const CRegionTextResult *result,
                                                         size_t page_index);

/**
 * Get a region's extracted UTF-8 text bytes. Returns `false` and zeroes `out`
 * for an invalid index or input pointer. The view remains valid until `result`
 * is freed.
 */
bool pdf_inspector_region_text_result_get_text(const CRegionTextResult *result,
                                               size_t page_index,
                                               size_t region_index,
                                               CByteView *out);

/**
 * Return whether a region's extracted text is unreliable and should be
 * replaced with OCR.
 */
bool pdf_inspector_region_text_result_needs_ocr(const CRegionTextResult *result,
                                                size_t page_index,
                                                size_t region_index);

/**
 * Get a region's optional machine-readable OCR-reason UTF-8 bytes. Returns
 * `false` and zeroes `out` when no reason is available or an input index or
 * pointer is invalid. The view remains valid until `result` is freed.
 */
bool pdf_inspector_region_text_result_get_ocr_reason(const CRegionTextResult *result,
                                                     size_t page_index,
                                                     size_t region_index,
                                                     CByteView *out);

/**
 * Free a `CVectorGridResult` instance. NULL is accepted.
 */
void pdf_inspector_vector_grid_result_free(CVectorGridResult *result);

/**
 * Return whether a grid was detected. A valid no-grid result returns false,
 * as does a NULL handle.
 */
bool pdf_inspector_vector_grid_result_is_detected(const CVectorGridResult *result);

/**
 * Get the number of HTML-like structure tokens in a detected grid. Returns
 * zero for a no-grid result or NULL handle.
 */
size_t pdf_inspector_vector_grid_result_get_structure_token_count(const CVectorGridResult *result);

/**
 * Get one borrowed UTF-8 structure token. Returns false and zeroes `out` for
 * a no-grid result, invalid index, NULL handle, or NULL output. The view stays
 * valid until the result handle is freed.
 */
bool pdf_inspector_vector_grid_result_get_structure_token(const CVectorGridResult *result,
                                                          size_t index,
                                                          CByteView *out);

/**
 * Get the number of crop-pixel cell boxes in a detected grid. Returns zero
 * for a no-grid result or NULL handle.
 */
size_t pdf_inspector_vector_grid_result_get_cell_count(const CVectorGridResult *result);

/**
 * Copy one detected cell box, in crop-image pixels with a top-left origin,
 * into `out`. Returns false and zeroes `out` for a no-grid result, malformed
 * or invalid index, NULL handle, or NULL output.
 */
bool pdf_inspector_vector_grid_result_get_cell_box(const CVectorGridResult *result,
                                                   size_t index,
                                                   CVectorGridCellBox *out);

/**
 * Free a `CTsrTableExtractionResult` instance. NULL is accepted.
 */
void pdf_inspector_tsr_result_free(CTsrTableExtractionResult *result);

/**
 * Get the number of table extraction results. Returns zero for a NULL handle.
 */
size_t pdf_inspector_tsr_result_get_table_count(const CTsrTableExtractionResult *result);

/**
 * Get one borrowed Markdown string. Returns false and zeroes `out` for an
 * invalid index, NULL handle, or NULL output. Empty Markdown is present.
 */
bool pdf_inspector_tsr_result_get_markdown(const CTsrTableExtractionResult *result,
                                           size_t index,
                                           CByteView *out);

/**
 * Get one optional borrowed fallback-reason label. Returns false and zeroes
 * `out` when no fallback occurred, or for an invalid index or NULL pointer.
 */
bool pdf_inspector_tsr_result_get_fallback_reason(const CTsrTableExtractionResult *result,
                                                  size_t index,
                                                  CByteView *out);

/**
 * Free a `CTsrStructuredCellsResult` instance. NULL is accepted.
 */
void pdf_inspector_tsr_cells_result_free(CTsrStructuredCellsResult *result);

/**
 * Get the number of input-parallel cell lists. Returns zero for a NULL handle.
 */
size_t pdf_inspector_tsr_cells_result_get_table_count(const CTsrStructuredCellsResult *result);

/**
 * Get the number of cells for one input. Returns zero for an invalid index or
 * NULL handle.
 */
size_t pdf_inspector_tsr_cells_result_get_cell_count(const CTsrStructuredCellsResult *result,
                                                     size_t table_index);

/**
 * Copy fixed metadata for one cell into `out`. Returns false and zeroes `out`
 * for invalid indices or NULL pointers.
 */
bool pdf_inspector_tsr_cells_result_get_cell(const CTsrStructuredCellsResult *result,
                                             size_t table_index,
                                             size_t cell_index,
                                             CTsrStructuredCell *out);

/**
 * Get one borrowed UTF-8 cell-text view. Returns false and zeroes `out` for
 * invalid indices or NULL pointers. Empty cell text is present. The view
 * remains valid until the result handle is freed.
 */
bool pdf_inspector_tsr_cells_result_get_cell_text(const CTsrStructuredCellsResult *result,
                                                  size_t table_index,
                                                  size_t cell_index,
                                                  CByteView *out);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* PDF_INSPECTOR_H */
