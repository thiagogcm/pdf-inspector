import java.io.IOException;
import java.io.InputStream;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * Extracts the Linux native library from the classpath (jar or directory) to
 * a temp file and loads it by absolute path. Must run before
 * the first touch of the generated pdf_inspector_h class: loaderLookup()
 * only finds libraries already loaded into the JVM when that class's static
 * initializer runs.
 *
 * Resource layout expected on the classpath: /native/libpdf_inspector.so.
 */
public final class PdfInspectorLoader {
    private static volatile boolean loaded = false;

    private PdfInspectorLoader() {}

    public static synchronized void load() {
        if (loaded) {
            return;
        }
        String resource = "/native/libpdf_inspector.so";
        try (InputStream in = PdfInspectorLoader.class.getResourceAsStream(resource)) {
            if (in == null) {
                throw new UnsatisfiedLinkError("Native library not found on classpath: " + resource);
            }
            Path tmp = Files.createTempFile("libpdf_inspector", ".so");
            tmp.toFile().deleteOnExit();
            Files.copy(in, tmp, StandardCopyOption.REPLACE_EXISTING);
            System.load(tmp.toAbsolutePath().toString());
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        loaded = true;
    }

}
