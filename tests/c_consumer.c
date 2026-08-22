#include "pdf_inspector.h"

#include <math.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Every enum here must stay 4 bytes under `-fshort-enums` too (see the
// `_ReservedMax` sentinels in pdf_inspector.h); scripts/test-c-consumer.sh
// compiles this file both ways.
_Static_assert(sizeof(PdfInspectorError) == 4, "PdfInspectorError must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(CPdfType) == 4, "CPdfType must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(CTextItemType) == 4, "CTextItemType must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(CProcessMode) == 4, "CProcessMode must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(CMarkdownProfile) == 4, "CMarkdownProfile must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(CScanStrategy) == 4, "CScanStrategy must stay 4 bytes under -fshort-enums");
_Static_assert(sizeof(COcrReason) == 4, "COcrReason must stay 4 bytes under -fshort-enums");

static int expect_error(const char *name, PdfInspectorError actual, PdfInspectorError expected) {
  if (actual == expected) {
    return 0;
  }
  fprintf(stderr, "%s: expected error %d, got %d\n", name, expected, actual);
  return 1;
}

int main(void) {
  CPdfProcessResult *result = NULL;
  CPagesExtractionResult *pages_result = NULL;
  CTextResult *text_result = NULL;
  CTextResult *markdown_result = NULL;
  CTextResult *items_markdown_result = NULL;
  CTextResult *built_markdown_result = NULL;
  CTextItemsResult *text_items = NULL;
  CTextItemsResult *built_items = NULL;
  CStructureElementsResult *structure_elements = NULL;
  CRegionTextResult *region_text = NULL;
  CRegionTextResult *region_tables = NULL;
  CVectorGridResult *vector_grid = NULL;
  CTsrTableExtractionResult *tsr_result = NULL;
  CTsrStructuredCellsResult *tsr_cells = NULL;
  CPdfClassification *classification = NULL;
  CPdfTypeResult *pdf_type_result = NULL;
  CPdfTypeResult *pdf_type_mem_result = NULL;
  CPdfOptions *options = NULL;
  int rc = 1;

  do {
    // The runtime value must match the header the consumer compiled against;
    // asserting a literal here would just have to be edited alongside any bump.
    if (pdf_inspector_abi_version() != PDF_INSPECTOR_ABI_VERSION) {
      break;
    }
    if (pdf_inspector_abi_minor() != PDF_INSPECTOR_ABI_MINOR) {
      break;
    }
    if (expect_error("process_pdf_mem with NULL data", pdf_inspector_process_pdf_mem(NULL, 1, NULL, &result), PdfInspectorError_NullPointer)) {
      break;
    }
    if (pdf_inspector_process_pdf_mem(NULL, 0, NULL, &result) == PdfInspectorError_NullPointer) {
      break;
    }

    // A NULL `path` must leave `*result_out` NULL even starting from a stale pointer.
    text_result = (CTextResult *)(void *)0x1;
    if (expect_error("extract_text with NULL path leaves out-param NULL", pdf_inspector_extract_text(NULL, NULL, &text_result), PdfInspectorError_NullPointer)) {
      break;
    }
    if (text_result != NULL) {
      fprintf(stderr, "extract_text with NULL path did not zero the out-parameter\n");
      break;
    }

    if (expect_error("extract_pages_markdown with NULL pages", pdf_inspector_extract_pages_markdown("tests/fixtures/bare_name_struct.pdf", NULL, 1, NULL, &pages_result), PdfInspectorError_InvalidArgument)) {
      break;
    }
    // Page numbers crossing this ABI are 1-indexed everywhere, so 0 is rejected.
    const uint32_t page_zero[] = {0};
    if (expect_error("extract_pages_markdown with page zero", pdf_inspector_extract_pages_markdown("tests/fixtures/bare_name_struct.pdf", page_zero, 1, NULL, &pages_result), PdfInspectorError_InvalidArgument)) {
      break;
    }
    const uint32_t page_one[] = {1};
    if (expect_error("extract_pages_markdown page one", pdf_inspector_extract_pages_markdown("tests/fixtures/bare_name_struct.pdf", page_one, 1, NULL, &pages_result), PdfInspectorError_Success) ||
        pdf_inspector_pages_result_get_entry_page_number(pages_result, 0) != 1) {
      break;
    }

    // The per-page routing surface: this is what a hybrid-OCR caller reads to
    // decide which pages to send to an OCR engine, so exercise all of it.
    CByteView page_markdown = {0};
    CU32View pages_needing_ocr = {0};
    CU32View pages_with_tables = {0};
    CU32View pages_with_columns = {0};
    if (pdf_inspector_pages_result_get_entry_count(pages_result) != 1 ||
        !pdf_inspector_pages_result_get_entry_markdown(pages_result, 0, &page_markdown) ||
        page_markdown.ptr == NULL ||
        // A text-based fixture needs no OCR, and reports no reason for one.
        pdf_inspector_pages_result_get_entry_needs_ocr(pages_result, 0) ||
        pdf_inspector_pages_result_get_entry_ocr_reason(pages_result, 0, &(CByteView){0}) ||
        pdf_inspector_pages_result_is_complex(pages_result) ||
        !pdf_inspector_pages_result_get_pages_needing_ocr(pages_result, &pages_needing_ocr) ||
        pages_needing_ocr.len != 0 ||
        !pdf_inspector_pages_result_get_pages_with_tables(pages_result, &pages_with_tables) ||
        !pdf_inspector_pages_result_get_pages_with_columns(pages_result, &pages_with_columns) ||
        pdf_inspector_pages_result_get_ocr_page_count(pages_result) != 0 ||
        // Out-of-range entries stay total: zero values, never a trap.
        pdf_inspector_pages_result_get_ocr_page_number(pages_result, 0) != 0 ||
        pdf_inspector_pages_result_get_ocr_page_reason_count(pages_result, 0) != 0 ||
        pdf_inspector_pages_result_get_ocr_page_reason(pages_result, 0, 0, &(CByteView){0})) {
      break;
    }

    // Reason strings map onto a switchable discriminant.
    const char scanned_reason[] = "scanned";
    const char nonsense_reason[] = "not-a-reason";
    if (pdf_inspector_ocr_reason_from_string((CByteView){(const uint8_t *)scanned_reason, sizeof(scanned_reason) - 1}) != COcrReason_Scanned ||
        pdf_inspector_ocr_reason_from_string((CByteView){(const uint8_t *)nonsense_reason, sizeof(nonsense_reason) - 1}) != COcrReason_Unknown ||
        pdf_inspector_ocr_reason_from_string((CByteView){NULL, 0}) != COcrReason_Unknown) {
      break;
    }

    if (expect_error("create options", pdf_inspector_options_new(&options), PdfInspectorError_Success) ||
        options == NULL ||
        expect_error("set negative text ratio", pdf_inspector_options_set_text_page_ratio_threshold(options, -0.1f), PdfInspectorError_InvalidArgument) ||
        expect_error("set NaN text ratio", pdf_inspector_options_set_text_page_ratio_threshold(options, NAN), PdfInspectorError_InvalidArgument) ||
        expect_error("set infinite text ratio", pdf_inspector_options_set_text_page_ratio_threshold(options, INFINITY), PdfInspectorError_InvalidArgument) ||
        expect_error("set valid text ratio", pdf_inspector_options_set_text_page_ratio_threshold(options, 0.5f), PdfInspectorError_Success) ||
        expect_error("set full process mode", pdf_inspector_options_set_mode(options, CProcessMode_Full), PdfInspectorError_Success) ||
        expect_error("set invalid process mode", pdf_inspector_options_set_mode(options, 99), PdfInspectorError_InvalidArgument) ||
        expect_error("set compact markdown profile", pdf_inspector_options_set_profile(options, CMarkdownProfile_Compact), PdfInspectorError_Success) ||
        expect_error("set invalid markdown profile", pdf_inspector_options_set_profile(options, -1), PdfInspectorError_InvalidArgument) ||
        expect_error("set fidelity markdown profile", pdf_inspector_options_set_profile(options, CMarkdownProfile_Fidelity), PdfInspectorError_Success) ||
        // Page 0 is rejected the same way at every page-list entry point.
        expect_error("add_page rejects page zero", pdf_inspector_options_add_page(options, 0), PdfInspectorError_InvalidArgument) ||
        expect_error("add_page accepts page one", pdf_inspector_options_add_page(options, 1), PdfInspectorError_Success) ||
        expect_error("clear pages", pdf_inspector_options_clear_pages(options), PdfInspectorError_Success) ||
        expect_error("set base font size", pdf_inspector_options_set_base_font_size(options, 11.0f), PdfInspectorError_Success) ||
        expect_error("clear base font size", pdf_inspector_options_set_base_font_size(options, 0.0f), PdfInspectorError_Success) ||
        expect_error("reject NaN base font size", pdf_inspector_options_set_base_font_size(options, NAN), PdfInspectorError_InvalidArgument) ||
        // Scan strategy: EarlyExit/Full take no payload; Sample needs a nonzero size.
        expect_error("set early-exit scan strategy", pdf_inspector_options_set_scan_strategy(options, CScanStrategy_EarlyExit, 0, NULL, 0), PdfInspectorError_Success) ||
        expect_error("set full scan strategy", pdf_inspector_options_set_scan_strategy(options, CScanStrategy_Full, 0, NULL, 0), PdfInspectorError_Success) ||
        expect_error("reject zero sample size", pdf_inspector_options_set_scan_strategy(options, CScanStrategy_Sample, 0, NULL, 0), PdfInspectorError_InvalidArgument) ||
        expect_error("set sample scan strategy", pdf_inspector_options_set_scan_strategy(options, CScanStrategy_Sample, 8, NULL, 0), PdfInspectorError_Success) ||
        expect_error("reject invalid scan strategy", pdf_inspector_options_set_scan_strategy(options, 99, 0, NULL, 0), PdfInspectorError_InvalidArgument)) {
      break;
    }

    if (expect_error("process PDF", pdf_inspector_process_pdf("tests/fixtures/bare_name_struct.pdf", options, &result), PdfInspectorError_Success) || result == NULL) {
      break;
    }

    const uint8_t markdown_input[] = {0xe2, 0x80, 0xa2, ' ', 'I', 't', 'e', 'm', '\n', 'a', '\0', 'b', '\n'};
    CByteView converted_markdown = {0};
    if (expect_error("convert plain text to Markdown", pdf_inspector_to_markdown(markdown_input, sizeof(markdown_input), NULL, &markdown_result), PdfInspectorError_Success) ||
        markdown_result == NULL ||
        !pdf_inspector_text_result_get_text(markdown_result, &converted_markdown) ||
        converted_markdown.ptr == NULL || converted_markdown.len != 11 ||
        memcmp(converted_markdown.ptr, "- Item\na\0b\n", 11) != 0) {
      break;
    }
    const uint8_t invalid_utf8[] = {0xff};
    CTextResult *invalid_markdown = (CTextResult *)(void *)0x1;
    if (expect_error("reject invalid Markdown input UTF-8", pdf_inspector_to_markdown(invalid_utf8, sizeof(invalid_utf8), NULL, &invalid_markdown), PdfInspectorError_InvalidUtf8) ||
        invalid_markdown != NULL) {
      break;
    }
    CByteView markdown = {0};
    if (!pdf_inspector_process_result_get_markdown(result, &markdown) || markdown.ptr == NULL || markdown.len < 6 || memcmp(markdown.ptr, "# Test", 6) != 0) {
      break;
    }

    CU32View detector_ocr_pages = {0};
    if (expect_error("detect PDF type", pdf_inspector_detect_pdf_type("tests/fixtures/bare_name_struct.pdf", options, &pdf_type_result), PdfInspectorError_Success) ||
        pdf_type_result == NULL ||
        pdf_inspector_pdf_type_result_get_type(pdf_type_result) != CPdfType_TextBased ||
        pdf_inspector_pdf_type_result_get_page_count(pdf_type_result) == 0 ||
        pdf_inspector_pdf_type_result_get_pages_sampled(pdf_type_result) == 0 ||
        pdf_inspector_pdf_type_result_get_pages_with_text(pdf_type_result) == 0 ||
        !(pdf_inspector_pdf_type_result_get_confidence(pdf_type_result) > 0.0f) ||
        !pdf_inspector_pdf_type_result_get_pages_needing_ocr(pdf_type_result, &detector_ocr_pages) ||
        detector_ocr_pages.len != 0 ||
        pdf_inspector_pdf_type_result_get_ocr_page_count(pdf_type_result) != 0) {
      break;
    }

    // Plain text comes back as a borrowed view, so a document whose text
    // contains a NUL byte survives instead of failing the whole call.
    CByteView text = {0};
    if (expect_error("extract text", pdf_inspector_extract_text("tests/fixtures/bare_name_struct.pdf", NULL, &text_result), PdfInspectorError_Success) ||
        text_result == NULL ||
        !pdf_inspector_text_result_get_text(text_result, &text) ||
        text.ptr == NULL || text.len == 0) {
      break;
    }

    const CRegion regions[] = {{0.0f, 0.0f, 2000.0f, 2000.0f}};
    const CPageRegions page_regions[] = {{1, regions, 1}};
    CByteView region_value = {0};
    if (expect_error("extract region text", pdf_inspector_extract_text_in_regions("tests/fixtures/bare_name_struct.pdf", page_regions, 1, NULL, &region_text), PdfInspectorError_Success) ||
        region_text == NULL ||
        pdf_inspector_region_text_result_get_entry_count(region_text) != 1 ||
        pdf_inspector_region_text_result_get_entry_page_number(region_text, 0) != 1 ||
        pdf_inspector_region_text_result_get_region_count(region_text, 0) != 1 ||
        !pdf_inspector_region_text_result_get_text(region_text, 0, 0, &region_value) ||
        region_value.ptr == NULL || region_value.len == 0 ||
        pdf_inspector_region_text_result_needs_ocr(region_text, 0, 0)) {
      break;
    }

    if (expect_error("extract region tables", pdf_inspector_extract_tables_in_regions("tests/fixtures/tnagriculture_06_12.pdf", page_regions, 1, NULL, &region_tables), PdfInspectorError_Success) ||
        region_tables == NULL ||
        !pdf_inspector_region_text_result_get_text(region_tables, 0, 0, &region_value) ||
        region_value.ptr == NULL || region_value.len == 0 ||
        pdf_inspector_region_text_result_needs_ocr(region_tables, 0, 0) ||
        memchr(region_value.ptr, '|', region_value.len) == NULL) {
      break;
    }

    const CRegion full_page = {0.0f, 0.0f, 612.0f, 792.0f};
    CVectorGridCellBox first_cell = {0};
    CByteView first_token = {0};
    if (expect_error("detect vector grid", pdf_inspector_detect_vector_grid_in_region("tests/fixtures/multiline_indent_cell_rect_grid.pdf", 30, &full_page, 200.0f, NULL, &vector_grid), PdfInspectorError_Success) ||
        vector_grid == NULL ||
        !pdf_inspector_vector_grid_result_is_detected(vector_grid) ||
        pdf_inspector_vector_grid_result_get_structure_token_count(vector_grid) == 0 ||
        !pdf_inspector_vector_grid_result_get_structure_token(vector_grid, 0, &first_token) ||
        first_token.len != 7 || memcmp(first_token.ptr, "<table>", 7) != 0 ||
        pdf_inspector_vector_grid_result_get_cell_count(vector_grid) == 0 ||
        !pdf_inspector_vector_grid_result_get_cell_box(vector_grid, 0, &first_cell) ||
        !(first_cell.x2 > first_cell.x1) || !(first_cell.y2 > first_cell.y1)) {
      break;
    }

    const CByteView tsr_tokens[] = {
        {(const uint8_t *)"<table>", 7}, {(const uint8_t *)"<thead>", 7},
        {(const uint8_t *)"<tr>", 4}, {(const uint8_t *)"<th></th>", 9},
        {(const uint8_t *)"<th></th>", 9}, {(const uint8_t *)"</tr>", 5},
        {(const uint8_t *)"</thead>", 8}, {(const uint8_t *)"<tbody>", 7},
        {(const uint8_t *)"<tr>", 4}, {(const uint8_t *)"<td></td>", 9},
        {(const uint8_t *)"<td></td>", 9}, {(const uint8_t *)"</tr>", 5},
        {(const uint8_t *)"</tbody>", 8}, {(const uint8_t *)"</table>", 8},
    };
    const float tsr_box_1[] = {10.0f, 7.0f, 100.0f, 18.0f};
    const float tsr_box_2[] = {110.0f, 7.0f, 200.0f, 18.0f};
    const float tsr_box_3[] = {10.0f, 35.0f, 100.0f, 60.0f};
    const float tsr_box_4[] = {110.0f, 35.0f, 200.0f, 60.0f};
    const CTsrCellBBox tsr_boxes[] = {
        {tsr_box_1, 4}, {tsr_box_2, 4}, {tsr_box_3, 4}, {tsr_box_4, 4},
    };
    const CTsrTableInput tsr_input = {
        4, {80.0f, 170.0f, 280.0f, 240.0f}, 72.0f,
        tsr_tokens, sizeof(tsr_tokens) / sizeof(tsr_tokens[0]),
        tsr_boxes, sizeof(tsr_boxes) / sizeof(tsr_boxes[0]),
    };
    const char expected_tsr[] = "|Department|Core Courses|\n|---|---|\n|BIO|8.23|\n";
    CByteView tsr_markdown = {0};
    CByteView tsr_reason = {(const uint8_t *)(void *)0x1, 1};
    if (expect_error("extract TSR table", pdf_inspector_extract_tables_with_structure_auto("tests/fixtures/bits_pilani_feedback.pdf", &tsr_input, 1, NULL, &tsr_result), PdfInspectorError_Success) ||
        tsr_result == NULL ||
        pdf_inspector_tsr_result_get_table_count(tsr_result) != 1 ||
        !pdf_inspector_tsr_result_get_markdown(tsr_result, 0, &tsr_markdown) ||
        tsr_markdown.len != sizeof(expected_tsr) - 1 ||
        memcmp(tsr_markdown.ptr, expected_tsr, sizeof(expected_tsr) - 1) != 0 ||
        pdf_inspector_tsr_result_get_fallback_reason(tsr_result, 0, &tsr_reason) ||
        tsr_reason.ptr != NULL || tsr_reason.len != 0) {
      break;
    }

    // The raw path renders the same structure without quality repair, so for a
    // well-formed token stream it must agree with the auto path exactly.
    CTsrTableExtractionResult *raw_tsr = NULL;
    CByteView raw_markdown = {0};
    int raw_rc = expect_error("extract raw TSR table", pdf_inspector_extract_tables_with_structure("tests/fixtures/bits_pilani_feedback.pdf", &tsr_input, 1, NULL, &raw_tsr), PdfInspectorError_Success);
    if (!raw_rc &&
        (pdf_inspector_tsr_result_get_table_count(raw_tsr) != 1 ||
         !pdf_inspector_tsr_result_get_markdown(raw_tsr, 0, &raw_markdown) ||
         raw_markdown.len != sizeof(expected_tsr) - 1 ||
         memcmp(raw_markdown.ptr, expected_tsr, sizeof(expected_tsr) - 1) != 0 ||
         // The raw path has no fallback to report, ever.
         pdf_inspector_tsr_result_get_fallback_reason(raw_tsr, 0, &(CByteView){0}))) {
      raw_rc = 1;
    }
    pdf_inspector_tsr_result_free(raw_tsr);
    if (raw_rc) {
      break;
    }

    CTsrStructuredCell structured_cell = {0};
    CByteView structured_text = {0};
    if (expect_error("extract TSR cells", pdf_inspector_extract_tables_with_structure_cells("tests/fixtures/bits_pilani_feedback.pdf", &tsr_input, 1, NULL, &tsr_cells), PdfInspectorError_Success) ||
        tsr_cells == NULL ||
        pdf_inspector_tsr_cells_result_get_table_count(tsr_cells) != 1 ||
        pdf_inspector_tsr_cells_result_get_cell_count(tsr_cells, 0) != 4 ||
        !pdf_inspector_tsr_cells_result_get_cell(tsr_cells, 0, 0, &structured_cell) ||
        structured_cell.row != 0 || structured_cell.col != 0 ||
        structured_cell.rowspan != 1 || structured_cell.colspan != 1 ||
        !structured_cell.is_header ||
        structured_cell.page_pt_bbox.x1 != 90.0f ||
        structured_cell.page_pt_bbox.y1 != 177.0f ||
        structured_cell.page_pt_bbox.x2 != 180.0f ||
        structured_cell.page_pt_bbox.y2 != 188.0f ||
        !pdf_inspector_tsr_cells_result_get_cell_text(tsr_cells, 0, 0, &structured_text) ||
        structured_text.len != 10 || memcmp(structured_text.ptr, "Department", 10) != 0) {
      break;
    }

    CByteView first_item_text = {0};
    if (expect_error("extract text with positions", pdf_inspector_extract_text_with_positions("tests/fixtures/firecrawl_docs_tagged.pdf", NULL, 0, NULL, &text_items), PdfInspectorError_Success) ||
        text_items == NULL ||
        pdf_inspector_text_items_result_get_count(text_items) == 0 ||
        !pdf_inspector_text_items_result_get_text(text_items, 0, &first_item_text) ||
        first_item_text.ptr == NULL) {
      break;
    }
    const size_t item_count = pdf_inspector_text_items_result_get_count(text_items);
    bool has_mcid = false;
    bool metrics_are_sane = item_count > 0;
    for (size_t i = 0; i < item_count; i++) {
      CTextItemMetrics metrics = {0};
      if (!pdf_inspector_text_items_result_get_metrics(text_items, i, &metrics) ||
          metrics.page == 0 || metrics.font_size <= 0.0f) {
        metrics_are_sane = false;
        break;
      }
      if (metrics.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID) {
        has_mcid = true;
      }
    }
    // An out-of-range index reports absence and zeroes the struct.
    CTextItemMetrics past_end = {0};
    past_end.page = 42;
    if (!has_mcid || !metrics_are_sane ||
        pdf_inspector_text_items_result_get_metrics(text_items, item_count, &past_end) ||
        past_end.page != 0) {
      break;
    }

    // Every extracted item carries its page-local font resource tag.
    CByteView first_font_tag = {0};
    if (!pdf_inspector_text_items_result_get_font_tag(text_items, 0, &first_font_tag) ||
        first_font_tag.ptr == NULL) {
      break;
    }

    CByteView items_markdown = {0};
    if (expect_error("convert positioned items to Markdown", pdf_inspector_to_markdown_from_items(text_items, NULL, 0, 0, options, &items_markdown_result), PdfInspectorError_Success) ||
        items_markdown_result == NULL ||
        !pdf_inspector_text_result_get_text(items_markdown_result, &items_markdown) ||
        items_markdown.ptr == NULL || items_markdown.len == 0 ||
        pdf_inspector_text_items_result_get_count(text_items) != item_count) {
      break;
    }

    // Caller-supplied positioned items: build a handle from scratch (the OCR
    // round-trip path) and feed it back through the Markdown converter.
    const char title_text[] = "Quarterly Report";
    const char body_text[] = "Revenue grew in every region.";
    const char link_text[] = "details";
    const char link_url[] = "https://example.com/report";
    const char ocr_font_tag[] = "Ocr0";
    const CByteView ocr_tag_view = {(const uint8_t *)ocr_font_tag, sizeof(ocr_font_tag) - 1};
    const CTextItemDescriptor built_descriptors[] = {
        {1, {(const uint8_t *)title_text, sizeof(title_text) - 1}, 72.0f, 720.0f, 200.0f, 24.0f,
         {NULL, 0}, {(const uint8_t *)ocr_font_tag, sizeof(ocr_font_tag) - 1}, 24.0f,
         CTextItemType_Text, {NULL, 0},
         PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD | PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID, 3},
        {1, {(const uint8_t *)body_text, sizeof(body_text) - 1}, 72.0f, 680.0f, 300.0f, 12.0f,
         {NULL, 0}, {NULL, 0}, 12.0f, CTextItemType_Text, {NULL, 0}, 0, 0},
        {1, {(const uint8_t *)link_text, sizeof(link_text) - 1}, 72.0f, 660.0f, 60.0f, 12.0f,
         {NULL, 0}, {NULL, 0}, 12.0f, CTextItemType_Link,
         {(const uint8_t *)link_url, sizeof(link_url) - 1}, 0, 0},
    };
    CByteView built_url = {0};
    CByteView built_tag = {0};
    CTextItemMetrics title_metrics = {0};
    CTextItemMetrics body_metrics = {0};
    CTextItemMetrics link_metrics = {0};
    if (expect_error("create caller-built items", pdf_inspector_text_items_result_new(&built_items), PdfInspectorError_Success) ||
        built_items == NULL ||
        pdf_inspector_text_items_result_get_count(built_items) != 0 ||
        expect_error("append caller-built items", pdf_inspector_text_items_result_add(built_items, built_descriptors, sizeof(built_descriptors) / sizeof(built_descriptors[0])), PdfInspectorError_Success) ||
        pdf_inspector_text_items_result_get_count(built_items) != 3 ||
        !pdf_inspector_text_items_result_get_metrics(built_items, 0, &title_metrics) ||
        !pdf_inspector_text_items_result_get_metrics(built_items, 1, &body_metrics) ||
        !pdf_inspector_text_items_result_get_metrics(built_items, 2, &link_metrics) ||
        // Everything written through a descriptor reads back unchanged.
        title_metrics.page != 1 || title_metrics.x != 72.0f || title_metrics.y != 720.0f ||
        title_metrics.width != 200.0f || title_metrics.height != 24.0f ||
        title_metrics.font_size != 24.0f ||
        title_metrics.item_type != CTextItemType_Text ||
        title_metrics.flags != (PDF_INSPECTOR_TEXT_ITEM_FLAG_BOLD |
                                PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID) ||
        title_metrics.mcid != 3 ||
        (body_metrics.flags & PDF_INSPECTOR_TEXT_ITEM_FLAG_HAS_MCID) != 0 ||
        link_metrics.item_type != CTextItemType_Link ||
        !pdf_inspector_text_items_result_get_font_tag(built_items, 0, &built_tag) ||
        built_tag.len != ocr_tag_view.len ||
        memcmp(built_tag.ptr, ocr_font_tag, ocr_tag_view.len) != 0 ||
        !pdf_inspector_text_items_result_get_link_url(built_items, 2, &built_url) ||
        built_url.len != sizeof(link_url) - 1 ||
        memcmp(built_url.ptr, link_url, sizeof(link_url) - 1) != 0) {
      break;
    }
    // Page 0 is rejected like every 1-indexed page crossing this ABI, and a
    // failed batch appends nothing.
    CTextItemDescriptor page_zero_descriptor = built_descriptors[1];
    page_zero_descriptor.page = 0;
    if (expect_error("reject caller-built item on page zero", pdf_inspector_text_items_result_add(built_items, &page_zero_descriptor, 1), PdfInspectorError_InvalidArgument) ||
        pdf_inspector_text_items_result_get_count(built_items) != 3) {
      break;
    }
    const CPdfRect built_rects[] = {{1, 70.0f, 650.0f, 320.0f, 30.0f}};
    CByteView built_markdown = {0};
    if (expect_error("convert caller-built items to Markdown", pdf_inspector_to_markdown_from_items(built_items, built_rects, 1, 2, NULL, &built_markdown_result), PdfInspectorError_Success) ||
        built_markdown_result == NULL ||
        !pdf_inspector_text_result_get_text(built_markdown_result, &built_markdown) ||
        built_markdown.ptr == NULL || built_markdown.len == 0) {
      break;
    }

    if (expect_error("extract structure elements", pdf_inspector_extract_structure_elements("tests/fixtures/firecrawl_docs_tagged.pdf", NULL, 0, NULL, &structure_elements), PdfInspectorError_Success) ||
        structure_elements == NULL ||
        pdf_inspector_structure_elements_result_get_count(structure_elements) == 0) {
      break;
    }
    const size_t element_count = pdf_inspector_structure_elements_result_get_count(structure_elements);
    bool has_h1 = false;
    for (size_t i = 0; i < element_count; i++) {
      CByteView role = {0};
      if (pdf_inspector_structure_elements_result_get_role(structure_elements, i, &role) && role.len == 2 && memcmp(role.ptr, "H1", 2) == 0) {
        has_h1 = true;
        break;
      }
    }
    if (!has_h1) {
      break;
    }

    if (expect_error("classify encrypted PDF without password", pdf_inspector_classify_pdf_mem((const uint8_t *)"", 0, NULL, &classification), PdfInspectorError_NotAPdf)) {
      break;
    }
    FILE *encrypted_file = fopen("tests/fixtures/encrypted-secret123.pdf", "rb");
    if (encrypted_file == NULL) {
      fprintf(stderr, "could not open tests/fixtures/encrypted-secret123.pdf\n");
      break;
    }
    fseek(encrypted_file, 0, SEEK_END);
    long encrypted_size = ftell(encrypted_file);
    fseek(encrypted_file, 0, SEEK_SET);
    if (encrypted_size < 0) {
      fclose(encrypted_file);
      fprintf(stderr, "could not determine size of tests/fixtures/encrypted-secret123.pdf\n");
      break;
    }
    uint8_t *encrypted_bytes = malloc((size_t)encrypted_size);
    if (encrypted_bytes == NULL) {
      fclose(encrypted_file);
      fprintf(stderr, "could not allocate memory for tests/fixtures/encrypted-secret123.pdf\n");
      break;
    }
    size_t read_bytes = fread(encrypted_bytes, 1, (size_t)encrypted_size, encrypted_file);
    fclose(encrypted_file);
    if (read_bytes != (size_t)encrypted_size) {
      free(encrypted_bytes);
      fprintf(stderr, "could not read tests/fixtures/encrypted-secret123.pdf\n");
      break;
    }

    int password_rc = 0;
    password_rc |= expect_error("classify encrypted PDF with wrong password", pdf_inspector_classify_pdf_mem(encrypted_bytes, (size_t)encrypted_size, "not-the-password", &classification), PdfInspectorError_Encrypted);
    password_rc |= expect_error("classify encrypted PDF with right password", pdf_inspector_classify_pdf_mem(encrypted_bytes, (size_t)encrypted_size, "secret123", &classification), PdfInspectorError_Success);
    password_rc |= expect_error("set detector password", pdf_inspector_options_set_password(options, "secret123"), PdfInspectorError_Success);
    password_rc |= expect_error("detect encrypted PDF bytes", pdf_inspector_detect_pdf_type_mem(encrypted_bytes, (size_t)encrypted_size, options, &pdf_type_mem_result), PdfInspectorError_Success);
    free(encrypted_bytes);
    if (password_rc || classification == NULL || pdf_type_mem_result == NULL ||
        pdf_inspector_pdf_type_result_get_page_count(pdf_type_mem_result) == 0) {
      break;
    }

    CU32View ocr_pages = {0};
    pdf_inspector_classification_get_pages_needing_ocr(classification, &ocr_pages);
    if (pdf_inspector_classification_get_confidence(classification) < 0.0f ||
        pdf_inspector_classification_get_confidence(classification) > 1.0f) {
      break;
    }

    // The path-based classifier mirrors the byte-based one.
    CPdfClassification *path_classification = NULL;
    int classify_rc = expect_error("classify_pdf by path", pdf_inspector_classify_pdf("tests/fixtures/bare_name_struct.pdf", NULL, &path_classification), PdfInspectorError_Success);
    if (!classify_rc &&
        (pdf_inspector_classification_get_type(path_classification) != CPdfType_TextBased ||
         pdf_inspector_classification_get_page_count(path_classification) == 0)) {
      classify_rc = 1;
    }
    pdf_inspector_classification_free(path_classification);
    if (classify_rc) {
      break;
    }

    // Document-level metadata on the full processing result.
    CU32View result_tables = {0};
    CU32View result_columns = {0};
    if (pdf_inspector_process_result_get_type(result) != CPdfType_TextBased ||
        pdf_inspector_process_result_get_page_count(result) == 0 ||
        pdf_inspector_process_result_get_confidence(result) < 0.0f ||
        pdf_inspector_process_result_get_confidence(result) > 1.0f ||
        pdf_inspector_process_result_has_encoding_issues(result) ||
        pdf_inspector_process_result_is_complex_layout(result) ||
        !pdf_inspector_process_result_get_pages_with_tables(result, &result_tables) ||
        !pdf_inspector_process_result_get_pages_with_columns(result, &result_columns) ||
        pdf_inspector_process_result_get_ocr_page_count(result) != 0 ||
        pdf_inspector_process_result_get_ocr_page_number(result, 0) != 0 ||
        pdf_inspector_process_result_get_ocr_page_reason_count(result, 0) != 0) {
      break;
    }
    // `processing_time_ms` is a duration, so only its readability is asserted.
    (void)pdf_inspector_process_result_get_processing_time_ms(result);

    // Byte-scan page estimate, without parsing the document.
    uint32_t estimated_pages = 0;
    const uint8_t estimate_garbage[] = "not a pdf";
    if (expect_error("estimate page count with NULL out", pdf_inspector_estimate_page_count_from_bytes(estimate_garbage, sizeof(estimate_garbage) - 1, NULL), PdfInspectorError_NullPointer) ||
        expect_error("estimate page count with NULL buffer", pdf_inspector_estimate_page_count_from_bytes(NULL, 1, &estimated_pages), PdfInspectorError_NullPointer) ||
        estimated_pages != 0 ||
        expect_error("estimate page count", pdf_inspector_estimate_page_count_from_bytes(estimate_garbage, sizeof(estimate_garbage) - 1, &estimated_pages), PdfInspectorError_Success)) {
      break;
    }

    // The copy-out diagnostic path: full length like snprintf, the originating
    // code, and no write past `cap`.
    {
      CPdfProcessResult *bad = NULL;
      const uint8_t junk[] = "definitely not a pdf";
      if (expect_error("process junk bytes", pdf_inspector_process_pdf_mem(junk, sizeof(junk) - 1, NULL, &bad), PdfInspectorError_NotAPdf)) {
        break;
      }
      int32_t code = -1;
      size_t need = pdf_inspector_last_error_copy(NULL, 0, &code);
      CByteView borrowed = {0};
      if (need == 0 || code != PdfInspectorError_NotAPdf ||
          !pdf_inspector_last_error_message(&borrowed) || borrowed.len != need) {
        break;
      }
      unsigned char small[8];
      memset(small, 0xAA, sizeof(small));
      // Clamp so this stays a truncation test regardless of the message length.
      size_t cap = need < sizeof(small) / 2 ? need : sizeof(small) / 2;
      if (pdf_inspector_last_error_copy(small, cap, NULL) != need ||
          memcmp(small, borrowed.ptr, cap) != 0 ||
          small[cap] != 0xAA) {  // nothing written past `cap`
        break;
      }
    }

    // A handle passed to the wrong `*_free` is refused, not freed. In C this
    // needs a cast to even compile -- which is the point: a binding generator
    // that erases the pointer types gets the same protection for free.
    {
      CTextResult *victim = NULL;
      CByteView still_there = {0};
      if (expect_error("extract text for wrong-free check", pdf_inspector_extract_text("tests/fixtures/bare_name_struct.pdf", NULL, &victim), PdfInspectorError_Success)) {
        break;
      }
      pdf_inspector_process_result_free((CPdfProcessResult *)(void *)victim);
      pdf_inspector_options_free((CPdfOptions *)(void *)victim);
      if (!pdf_inspector_text_result_get_text(victim, &still_there) ||
          still_there.len == 0) {
        pdf_inspector_text_result_free(victim);
        break;
      }
      pdf_inspector_text_result_free(victim);
    }

    // No message after a success.
    CByteView last_error = {0};
    if (pdf_inspector_last_error_message(&last_error)) {
      fprintf(stderr, "last-error message should be NULL after a successful call\n");
      break;
    }
    CPdfProcessResult *not_a_pdf_result = NULL;
    const uint8_t garbage[] = "not a pdf";
    if (expect_error("process garbage bytes", pdf_inspector_process_pdf_mem(garbage, sizeof(garbage) - 1, NULL, &not_a_pdf_result), PdfInspectorError_NotAPdf)) {
      break;
    }
    if (!pdf_inspector_last_error_message(&last_error) || last_error.ptr == NULL || last_error.len == 0) {
      fprintf(stderr, "expected a last-error message after PdfInspectorError_NotAPdf\n");
      break;
    }

    rc = 0;
  } while (0);

  // Every free tolerates NULL, so one unconditional block covers all exits.
  pdf_inspector_classification_free(classification);
  pdf_inspector_pdf_type_result_free(pdf_type_mem_result);
  pdf_inspector_pdf_type_result_free(pdf_type_result);
  pdf_inspector_tsr_cells_result_free(tsr_cells);
  pdf_inspector_tsr_result_free(tsr_result);
  pdf_inspector_vector_grid_result_free(vector_grid);
  pdf_inspector_structure_elements_result_free(structure_elements);
  pdf_inspector_region_text_result_free(region_tables);
  pdf_inspector_region_text_result_free(region_text);
  pdf_inspector_text_items_result_free(built_items);
  pdf_inspector_text_items_result_free(text_items);
  pdf_inspector_text_result_free(built_markdown_result);
  pdf_inspector_text_result_free(items_markdown_result);
  pdf_inspector_text_result_free(markdown_result);
  pdf_inspector_text_result_free(text_result);
  pdf_inspector_pages_result_free(pages_result);
  pdf_inspector_process_result_free(result);
  pdf_inspector_options_free(options);
  return rc;
}
