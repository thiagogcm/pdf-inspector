#include "pdf_inspector.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

_Static_assert(sizeof(((PdfRequest *)0)->outputs) == 4, "flags have a fixed width");
_Static_assert(sizeof(((PdfRequest *)0)->flags) == 4, "request flags have a fixed width");
_Static_assert(sizeof(((PdfError *)0)->status) == 4, "status has a fixed width");
_Static_assert(sizeof(((PdfItem *)0)->mcid) == 8, "MCID has a fixed width");
_Static_assert(sizeof(((PdfItem *)0)->dest_page) == 4, "dest_page has a fixed width");
_Static_assert(sizeof(((PdfLoadAudit *)0)->flags) == 4, "audit flags have a fixed width");
_Static_assert(sizeof(((PdfCell *)0)->row) == 4, "cell indices have a fixed width");

static PdfBytes bytes(const char *s) {
    PdfBytes result = {(const uint8_t *)s, strlen(s)};
    return result;
}
static int contains(PdfBytes value, const char *needle) {
    size_t length = strlen(needle);
    if (!value.ptr || length > value.len) return 0;
    for (size_t i = 0; i <= value.len - length; ++i) {
        if (!memcmp(value.ptr + i, needle, length)) return 1;
    }
    return 0;
}
#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "C ABI check failed at line %d: %s\n", __LINE__, #condition); goto cleanup; } } while (0)

int main(void) {
    int exit_code = 1;
    PdfDocument *document = NULL;
    PdfResult *first = NULL, *second = NULL, *composed = NULL;
    PdfError *error = NULL, *earlier_error = NULL;
    PdfRequest request;
    PdfMarkdownOptions markdown_options;
    PdfComposeInput compose = {0};
    PdfSource source = {PDF_SOURCE_PATH, {0}, {0}};
    source.data = bytes("../tests/fixtures/bare_name_struct.pdf");

    CHECK(pdf_inspector_version().len > 0);
    CHECK(pdf_inspector_capabilities() & PDF_CAP_EXTERNAL_OCR);
    CHECK(pdf_inspector_request_init(&request) == PDF_OK);
    CHECK(request.render.flags == (PDF_RENDER_ANNOTATIONS | PDF_RENDER_FORM_FIELDS));
    CHECK(pdf_inspector_open(&source, &document, &error) == PDF_OK);
    CHECK(document && !error);
    const PdfDocumentInfo *info = pdf_inspector_document_info(document);
    CHECK(info->page_count == 1 && info->pages.len == 1 && info->pages.ptr[0].page == 1);
    CHECK(info->audit.leading_bytes == 0);
    request.outputs |= PDF_OUT_ITEMS | PDF_OUT_STRUCTURE | PDF_OUT_GEOMETRY | PDF_OUT_TEXT;
    CHECK(pdf_inspector_execute(document, &request, &first, &error) == PDF_OK);
    CHECK(first && !error);
    CHECK(first->page_count == 1 && first->pages.len == 1);
    CHECK(contains(first->markdown, "# Test"));
    CHECK(contains(first->text, "Test"));
    const PdfPage *page = &first->pages.ptr[0];
    CHECK(page->info.page == 1 && page->items.len > 0 && page->structure.len > 0);
    CHECK((first->present & PDF_OUT_ANALYSIS) && first->structure_nodes.len > 0);
    CHECK(first->structure_nodes.ptr[0].parent == 0);
    CHECK(page->items.ptr[0].bounds.y1 >= page->items.ptr[0].bounds.y0);
    CHECK(page->text_orientation == PDF_ORIENTATION_UPRIGHT);

    request.markdown.flags &= ~PDF_MD_HEADERS;
    CHECK(pdf_inspector_execute(document, &request, &second, &error) == PDF_OK);
    CHECK(!contains(second->markdown, "# Test"));
    CHECK(contains(first->markdown, "# Test"));
    pdf_inspector_result_free(second); second = NULL;

    request.ocr.minimum_confidence = NAN;
    CHECK(pdf_inspector_execute(document, &request, &second, &earlier_error) == PDF_INVALID_ARGUMENT);
    CHECK(!second && earlier_error);
    CHECK(earlier_error->status == PDF_INVALID_ARGUMENT && earlier_error->message.len > 0);
    PdfBytes saved_message = earlier_error->message;
    request.ocr.minimum_confidence = 0.0f;
    request.outputs = UINT32_MAX;
    CHECK(pdf_inspector_execute(document, &request, &second, &error) == PDF_INVALID_ARGUMENT);
    CHECK(!second && error);
    CHECK(contains(saved_message, "confidence"));
    pdf_inspector_error_free(error); error = NULL;

    PdfRuntimeOptions runtime;
    PdfRuntimeInfo runtime_info;
    CHECK(pdf_inspector_runtime_options_init(&runtime) == PDF_OK);
    CHECK(runtime.download_policy == PDF_DOWNLOAD_OFFLINE);
    runtime.capabilities = PDF_CAP_EXTERNAL_OCR;
    CHECK(pdf_inspector_prepare_runtime(&runtime, &runtime_info, &error) == PDF_INVALID_ARGUMENT);
    CHECK(runtime_info.capabilities == 0 && error);
    pdf_inspector_error_free(error); error = NULL;

    CHECK(pdf_inspector_markdown_options_init(&markdown_options) == PDF_OK);
    const uint8_t text[] = {'a', 0, 'b'};
    PdfBytes plain = {text, sizeof(text)};
    CHECK(pdf_inspector_compose_text(plain, &markdown_options, &composed, &error) == PDF_OK);
    CHECK(composed->markdown.len >= sizeof(text) && !memcmp(composed->markdown.ptr, text, sizeof(text)));
    pdf_inspector_result_free(composed); composed = NULL;

    compose.pages.ptr = &page->info; compose.pages.len = 1;
    compose.items = page->items; compose.structure = page->structure;
    compose.rectangles = page->rectangles; compose.lines = page->lines;
    CHECK(pdf_inspector_compose_items(&compose, NULL, &composed, &error) == PDF_OK);
    CHECK(contains(composed->markdown, "Test"));

    pdf_inspector_document_free(document); document = NULL;
    CHECK(contains(first->markdown, "# Test"));
    CHECK(contains(saved_message, "confidence"));
    CHECK(!pdf_inspector_document_info(NULL));
    exit_code = 0;
cleanup:
    if (error) {
        fwrite(error->message.ptr, 1, error->message.len, stderr);
        fputc('\n', stderr);
    }
    pdf_inspector_error_free(error);
    pdf_inspector_error_free(earlier_error);
    pdf_inspector_result_free(first);
    pdf_inspector_result_free(second);
    pdf_inspector_result_free(composed);
    pdf_inspector_document_free(document);
    return exit_code;
}
