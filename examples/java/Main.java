import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;

import com.pdfinspector.ffi.CByteView;
import com.pdfinspector.ffi.CU32View;
import com.pdfinspector.ffi.pdf_inspector_h;

/**
 * End-to-end example: process a PDF, read its Markdown, read a u32 array
 * output, then read positioned text items. Demonstrates the FFM idioms the
 * ABI needs: T** out-params, CByteView/CU32View out-views, and per-index
 * getters into an opaque handle.
 *
 * Run with: java --enable-native-access=ALL-UNNAMED -cp classes Main <pdf-path>
 */
public class Main {
    public static void main(String[] args) throws Exception {
        PdfInspectorLoader.load();
        String path = args.length > 0 ? args[0] : "tests/fixtures/2013-app2.pdf";

        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cPath = arena.allocateFrom(path);

            // --- T** out-parameter pattern ---
            MemorySegment options = pdf_inspector_h.pdf_inspector_options_new();
            MemorySegment result = MemorySegment.NULL;
            try {
                int rc = pdf_inspector_h.pdf_inspector_options_set_mode(
                        options, pdf_inspector_h.CProcessMode_Full());
                checkSuccess("options_set_mode", rc);

                MemorySegment resultOut = arena.allocate(ValueLayout.ADDRESS);
                rc = pdf_inspector_h.pdf_inspector_process_pdf(cPath, options, resultOut);
                checkSuccess("process_pdf", rc);
                result = resultOut.get(ValueLayout.ADDRESS, 0);

                // --- borrowed CByteView UTF-8 string (NOT NUL-terminated) ---
                MemorySegment markdownView = CByteView.allocate(arena);
                if (!pdf_inspector_h.pdf_inspector_result_get_markdown(result, markdownView)) {
                    throw new RuntimeException("result has no Markdown");
                }
                String markdown = readUtf8(markdownView);
                System.out.println("Markdown: " + markdown.length() + " chars, "
                        + pdf_inspector_h.pdf_inspector_result_get_page_count(result) + " pages");

                // --- borrowed CU32View u32 array ---
                MemorySegment pagesView = CU32View.allocate(arena);
                if (!pdf_inspector_h.pdf_inspector_result_get_pages_with_tables(result, pagesView)) {
                    throw new RuntimeException("result has no pages-with-tables view");
                }
                int[] pagesWithTables = readU32(pagesView);
                System.out.println("Pages with tables: " + java.util.Arrays.toString(pagesWithTables));
            } finally {
                if (!result.equals(MemorySegment.NULL)) {
                    pdf_inspector_h.pdf_inspector_result_free(result);
                }
                pdf_inspector_h.pdf_inspector_options_free(options);
            }

            // --- positioned text items: per-index getters into an opaque handle ---
            MemorySegment itemsOut = arena.allocate(ValueLayout.ADDRESS);
            MemorySegment items = MemorySegment.NULL;
            try {
                int rc = pdf_inspector_h.pdf_inspector_extract_text_with_positions(
                        cPath, MemorySegment.NULL, 0, MemorySegment.NULL, itemsOut);
                checkSuccess("extract_text_with_positions", rc);
                items = itemsOut.get(ValueLayout.ADDRESS, 0);
                long count = pdf_inspector_h.pdf_inspector_text_items_result_get_count(items);
                System.out.println("Text items: " + count);
                if (count > 0) {
                    MemorySegment textView = CByteView.allocate(arena);
                    if (!pdf_inspector_h.pdf_inspector_text_items_result_get_text(items, 0, textView)) {
                        throw new RuntimeException("item 0 has no text view");
                    }
                    System.out.println("Item 0: \"" + readUtf8(textView) + "\" at ("
                            + pdf_inspector_h.pdf_inspector_text_items_result_get_x(items, 0) + ", "
                            + pdf_inspector_h.pdf_inspector_text_items_result_get_y(items, 0) + ")");
                }
            } finally {
                if (!items.equals(MemorySegment.NULL)) {
                    pdf_inspector_h.pdf_inspector_text_items_result_free(items);
                }
            }
        }
    }

    static void checkSuccess(String operation, int rc) {
        if (rc != pdf_inspector_h.PdfInspectorError_Success()) {
            throw new RuntimeException(operation + " failed: " + rc);
        }
    }

    /** Reads a borrowed CByteView. The view remains valid only while its handle lives. */
    static String readUtf8(MemorySegment view) {
        return readUtf8(CByteView.ptr(view), CByteView.len(view));
    }

    /**
     * Reads a borrowed UTF-8 byte range. Never use getString()/strlen here:
     * the ABI's strings are not NUL-terminated and may contain interior NULs.
     */
    static String readUtf8(MemorySegment ptr, long len) {
        if (ptr.equals(MemorySegment.NULL) || len == 0) {
            return "";
        }
        return new String(ptr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
    }

    /** Reads a borrowed CU32View without dereferencing its pointer when empty. */
    static int[] readU32(MemorySegment view) {
        long len = CU32View.len(view);
        if (len == 0) {
            return new int[0];
        }
        MemorySegment ptr = CU32View.ptr(view);
        if (ptr.equals(MemorySegment.NULL)) {
            throw new IllegalStateException("non-empty CU32View has a NULL pointer");
        }
        long byteSize = Math.multiplyExact(len, ValueLayout.JAVA_INT.byteSize());
        return ptr.reinterpret(byteSize).toArray(ValueLayout.JAVA_INT);
    }
}
