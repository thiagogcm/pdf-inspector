#include "pdf_inspector.h"
#include <math.h>
#include <stdio.h>
#include <string.h>

_Static_assert(sizeof(((PdfRequest *)0)->outputs) == 4, "flags have a fixed width");
_Static_assert(sizeof(((PdfDiagnostic *)0)->status) == 4, "status has a fixed width");
_Static_assert(sizeof(((PdfItem *)0)->mcid) == 8, "MCID has a fixed width");

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
    PdfErrorHandle *error = NULL, *earlier_error = NULL;
    PdfRequest request;
    PdfComposeInput compose;
    PdfSource source = {PDF_SOURCE_PATH, {0}};
    source.data = bytes("../tests/fixtures/bare_name_struct.pdf");

    CHECK(pdf_inspector_capabilities() & PDF_CAP_EXTERNAL_OCR);
    CHECK(pdf_inspector_request_init(&request) == PDF_OK);
    CHECK(pdf_inspector_open(&source, NULL, &document, &error) == PDF_OK);
    CHECK(document && !error);
    request.outputs |= PDF_ITEMS | PDF_STRUCTURE | PDF_GEOMETRY | PDF_TEXT;
    CHECK(pdf_inspector_execute(document, &request, &first, &error) == PDF_OK);
    CHECK(first && !error);
    const PdfResultView *view = pdf_inspector_result_view(first);
    CHECK(view->page_count == 1 && view->pages.len == 1);
    CHECK(contains(view->markdown, "# Test"));
    CHECK(contains(view->text, "Test"));
    const PdfPage *page = &view->pages.ptr[0];
    CHECK(page->info.page == 1 && page->items.len > 0 && page->structure.len > 0);
    CHECK((view->present & PDF_ANALYSIS) && view->structure_nodes.len > 0);
    CHECK(view->structure_nodes.ptr[0].id == 1 && view->structure_nodes.ptr[0].parent == 0);
    CHECK(page->items.ptr[0].bounds.y1 >= page->items.ptr[0].bounds.y0);

    request.markdown.flags &= ~PDF_MD_HEADERS;
    CHECK(pdf_inspector_execute(document, &request, &second, &error) == PDF_OK);
    CHECK(!contains(pdf_inspector_result_view(second)->markdown, "# Test"));
    CHECK(contains(view->markdown, "# Test"));
    pdf_inspector_result_free(second); second = NULL;

    request.ocr.minimum_confidence = NAN;
    CHECK(pdf_inspector_execute(document, &request, &second, &earlier_error) == PDF_INVALID_ARGUMENT);
    CHECK(!second && earlier_error);
    const PdfDiagnostic *diagnostic = pdf_inspector_error_view(earlier_error);
    CHECK(diagnostic->status == PDF_INVALID_ARGUMENT && diagnostic->message.len > 0);
    PdfBytes saved_message = diagnostic->message;
    request.ocr.minimum_confidence = 0.0f;
    request.outputs = UINT32_MAX;
    CHECK(pdf_inspector_execute(document, &request, &second, &error) == PDF_INVALID_ARGUMENT);
    CHECK(!second && error);
    CHECK(contains(saved_message, "confidence"));
    pdf_inspector_error_free(error); error = NULL;

    PdfRuntimeOptions runtime;
    CHECK(pdf_inspector_runtime_options_init(&runtime) == PDF_OK);
    CHECK(runtime.download_policy == PDF_DOWNLOAD_OFFLINE);
    runtime.capabilities = PDF_CAP_EXTERNAL_OCR;
    CHECK(pdf_inspector_prepare_runtime(&runtime, &second, &error) == PDF_INVALID_ARGUMENT);
    CHECK(!second && error);
    pdf_inspector_error_free(error); error = NULL;

    CHECK(pdf_inspector_compose_init(&compose) == PDF_OK);
    const uint8_t text[] = {'a', 0, 'b'};
    compose.text.ptr = text; compose.text.len = sizeof(text);
    CHECK(pdf_inspector_compose(&compose, &composed, &error) == PDF_OK);
    PdfBytes markdown = pdf_inspector_result_view(composed)->markdown;
    CHECK(markdown.len >= sizeof(text) && !memcmp(markdown.ptr, text, sizeof(text)));
    pdf_inspector_result_free(composed); composed = NULL;

    compose.kind = PDF_COMPOSE_ITEMS;
    compose.pages.ptr = &page->info; compose.pages.len = 1;
    compose.items = page->items; compose.structure = page->structure;
    compose.rectangles = page->rectangles; compose.lines = page->lines;
    CHECK(pdf_inspector_compose(&compose, &composed, &error) == PDF_OK);
    CHECK(contains(pdf_inspector_result_view(composed)->markdown, "Test"));

    pdf_inspector_document_free(document); document = NULL;
    CHECK(contains(view->markdown, "# Test"));
    CHECK(contains(saved_message, "confidence"));
    CHECK(!pdf_inspector_result_view(NULL) && !pdf_inspector_error_view(NULL));
    exit_code = 0;
cleanup:
    if (error) {
        const PdfDiagnostic *diagnostic = pdf_inspector_error_view(error);
        fwrite(diagnostic->message.ptr, 1, diagnostic->message.len, stderr);
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
