# Java Bindings

Java bindings over the [C ABI](c-api.md) using the JDK's Foreign Function & Memory API (FFM, [JEP 454](https://openjdk.org/jeps/454)), generated with [jextract](https://github.com/openjdk/jextract). This document covers **packaging and code generation mechanics only**; ownership rules, handle lifetimes, and the function surface are documented in [`docs/c-api.md`](c-api.md); read that first.

Every command and number below was run on this machine and is reproducible:

| Tool     | Version                                    |
| -------- | ------------------------------------------ |
| JDK      | 25.0.4+7-LTS (Temurin)                     |
| jextract | 25 (bundled LibClang: Ubuntu clang 20.1.2) |

jextract requires JDK ≥ 22 (FFM finalized in JDK 22, [JEP 454](https://openjdk.org/jeps/454)); the restricted-methods enforcement discussed in [§4](#4-jep-472-restricted-methods) is [JEP 472](https://openjdk.org/jeps/472), current through JDK 25 and Preview-flagged for removal in a future release. Build the native library first:

```bash
cargo build --release --lib --features c-api
```

A runnable example (loader, `Main.java`, filter file, build script) lives in [`examples/java/`](../examples/java/); it is exercised as part of verifying this document and produces the output shown in [§8](#8-complete-example).

## 1. Generating bindings: do not pass `--library`

```bash
jextract --output src/main/java -t com.pdfinspector.ffi @examples/java/filter.txt pdf_inspector.h
```

The single most important detail: **omit `--library`.** With `--library <name>`, jextract emits

```java
static final SymbolLookup SYMBOL_LOOKUP = SymbolLookup.libraryLookup(System.mapLibraryName("pdf_inspector"), LIBRARY_ARENA)
        .or(SymbolLookup.loaderLookup())
        .or(Linker.nativeLinker().defaultLookup());
```

`.or(...)` is a method call on the _result_ of `libraryLookup(...)`; `libraryLookup` throws immediately if it can't resolve the library by its bare mapped name (`libpdf_inspector.so`) on the default search path. The fallbacks are never reached. Without `--library`, jextract instead emits `SymbolLookup.loaderLookup().or(defaultLookup())`, which finds a library already loaded into the JVM by any means, including `System.load(absolutePath)`, which is what a jar-packaged native library needs. Verified with all four combinations, calling `pdf_inspector_abi_version()`:

| Generated with | Loading strategy | Result |
| --- | --- | --- |
| `--library` | `System.load(abs)`, no `LD_LIBRARY_PATH` | **Fails.** `ExceptionInInitializerError` ← `IllegalArgumentException: Cannot open library: libpdf_inspector.so` at `SymbolLookup.libraryLookup` |
| `--library` | `LD_LIBRARY_PATH` set | Works |
| _(omitted)_ | `System.load(abs)`, no `LD_LIBRARY_PATH` | Works |
| _(omitted)_ | no load at all | Fails. `NoSuchElementException: Symbol not found` |

`--use-system-load-library` is a **trap, not a fix**: it only takes effect combined with `--library`, and it replaces the lookup chain's initializer with `static { System.loadLibrary("pdf_inspector"); }`. `System.loadLibrary` searches `java.library.path` by short name; it hard-fails with `UnsatisfiedLinkError: no pdf_inspector in java.library.path: ...` **even when the caller already ran `System.load(absolutePath)`** first, because the static initializer runs its own unconditional lookup regardless of what the JVM already has loaded. Verified: same steps as the first table row, plus `--use-system-load-library`, still fails with that `UnsatisfiedLinkError`.

## 2. Trimming the generated surface

Unfiltered, jextract also generates bindings for everything pulled in through `<stdbool.h>`, `<stdint.h>`, and transitively `<features.h>`, `<bits/*.h>`, glibc's own typedefs, macros, and limits. Measured against the current header (149 exported functions):

|  | Unfiltered (`--library`, no filter) | Filtered (`@filter.txt`) |
| --- | --- | --- |
| Lines (`pdf_inspector_h.java`) | 11145 | 9534 |
| `public static` members | 1295 | 1085 |
| ...of which not `pdf_inspector_*`-prefixed | glibc typedefs, macros, limits, and booleans | 42 (the ABI's own flags, error codes, enum constants, and ABI-version macros) |
| Extra generated `.java` files | `max_align_t.java`, `__fsid_t.java` | _(gone)_ |
| jextract warnings | 13 | 12 |

(These counts move as the header grows; rerun the commands below to get current numbers. The shape of the table, not the exact figures, is what per-header re-verification should reproduce.)

That leakage adds generated bindings for system-header details that are not part of this ABI's contract. The fix:

```bash
jextract --dump-includes filter.txt pdf_inspector.h
```

`--dump-includes` writes one `--include-*` directive per extractable symbol, grouped by the header it came from, e.g.:

```
#### Extracted from: /path/to/pdf_inspector.h

--include-constant CPdfType_TextBased  # header: /path/to/pdf_inspector.h
--include-struct CTextItemsResult      # header: /path/to/pdf_inspector.h
...

#### Extracted from: /usr/include/features.h
...
```

Keep only the block under `pdf_inspector.h`'s own `#### Extracted from:` header, strip the trailing `# header: ...` comments (they hardcode the machine's absolute path), and pass the result with `@`:

```bash
jextract --dump-includes /tmp/dump.txt pdf_inspector.h
sed -n '/pdf_inspector\.h$/,/^####/p' /tmp/dump.txt | grep '^--' > filter.txt   # keep only this header's block
jextract --output src/main/java -t com.pdfinspector.ffi @filter.txt pdf_inspector.h
```

[`examples/java/filter.txt`](../examples/java/filter.txt) is the checked-in result; regenerate it whenever functions, constants, or public types are added to `pdf_inspector.h`. The twelve opaque-handle typedefs (`CTextItemsResult`, `CPdfTypeResult`, `PdfOptions`, ...) _do_ appear in the dump as `--include-struct` entries and belong in the filter; see [§5](#5-the-opaque-handle-warnings-are-expected) for why they still produce a "Skipping" warning.

## 3. Loading and packaging

FFM's `loaderLookup()` ignores `-Djava.library.path` entirely. With the `--library` option omitted, extract the Linux native library from the jar to a temp file, then call `System.load(absolutePath)` **before** the generated class is first touched. The lookup chain is computed once, in a static initializer.

[`examples/java/PdfInspectorLoader.java`](../examples/java/PdfInspectorLoader.java):

```java
public static synchronized void load() {
    if (loaded) return;
    String resource = "/native/libpdf_inspector.so";
    try (InputStream in = PdfInspectorLoader.class.getResourceAsStream(resource)) {
        if (in == null) throw new UnsatisfiedLinkError("Native library not found on classpath: " + resource);
        Path tmp = Files.createTempFile("libpdf_inspector", ".so");
        tmp.toFile().deleteOnExit();
        Files.copy(in, tmp, StandardCopyOption.REPLACE_EXISTING);
        System.load(tmp.toAbsolutePath().toString());
    } catch (IOException e) {
        throw new UncheckedIOException(e);
    }
    loaded = true;
}
```

Call `PdfInspectorLoader.load()` as the first line of `main`, before any reference to the generated `pdf_inspector_h` class (a static field access or method call both trigger class initialization).

**Jar layout**:

```
myapp.jar
├── com/pdfinspector/ffi/...          (jextract output)
├── com/pdfinspector/PdfInspectorLoader.class
└── native/
    └── libpdf_inspector.so
```

The native library carries SONAME `libpdf_inspector.so.1`, tracking `PDF_INSPECTOR_ABI_VERSION`. This is a C packaging concern. The Java loader resolves the extracted file by absolute path.

## 4. JEP 472 restricted methods

On JDK 25, native access without `--enable-native-access` prints a runtime warning. It still runs today, but the JDK 25 warning text says restricted methods "will be blocked in a future release." Which API is named in the warning depends on which restricted call your code reaches first. With the recommended `System.load(absolutePath)` loader above, that's `System.load` itself:

```
WARNING: A restricted method in java.lang.System has been called
WARNING: java.lang.System::load has been called by MainC in an unnamed module (file:...)
WARNING: Use --enable-native-access=ALL-UNNAMED to avoid a warning for callers in this module
WARNING: Restricted methods will be blocked in a future release unless native access is enabled
```

If the library was already loaded some other way (e.g. `--library` + `LD_LIBRARY_PATH`), the first restricted call instead comes from jextract's own generated code, at the `$shared` class's C_POINTER initialization:

```
WARNING: A restricted method in java.lang.foreign.AddressLayout has been called
WARNING: java.lang.foreign.AddressLayout::withTargetLayout has been called by com.pdfinspector.ffi.pdf_inspector_h$shared in an unnamed module (file:...)
```

Both are silenced the same way. Unnamed-module (classpath) form:

```bash
java --enable-native-access=ALL-UNNAMED -cp classes Main
```

Named-module (JPMS) form, naming the module instead of `ALL-UNNAMED`:

```bash
java --enable-native-access=com.example.app -p out -m com.example.app/com.example.app.Main
```

For an executable jar, set it in the manifest instead of requiring the flag on every invocation (verified with `java -jar app.jar`, no `LD_LIBRARY_PATH`, no command-line flag):

```
Main-Class: com.pdfinspector.Main
Enable-Native-Access: ALL-UNNAMED
```

## 5. The opaque-handle warnings are expected

`jextract ... pdf_inspector.h` exits **0** with exactly **13 warnings** unfiltered, or **12** with the checked-in filter (the system-header warning is removed):

```
pdf_inspector.h:144:16: warning: Skipping CPagesExtractionResult (type Declared(CPagesExtractionResult) is not supported)
pdf_inspector.h:151:16: warning: Skipping CPdfClassification (type Declared(CPdfClassification) is not supported)
pdf_inspector.h:156:16: warning: Skipping CPdfProcessResult (type Declared(CPdfProcessResult) is not supported)
pdf_inspector.h:162:16: warning: Skipping CPdfTypeResult (type Declared(CPdfTypeResult) is not supported)
pdf_inspector.h:167:16: warning: Skipping CRegionTextResult (type Declared(CRegionTextResult) is not supported)
pdf_inspector.h:172:16: warning: Skipping CStructureElementsResult (type Declared(CStructureElementsResult) is not supported)
pdf_inspector.h:177:16: warning: Skipping CTextItemsResult (type Declared(CTextItemsResult) is not supported)
pdf_inspector.h:182:16: warning: Skipping CTextResult (type Declared(CTextResult) is not supported)
pdf_inspector.h:188:16: warning: Skipping CTsrStructuredCellsResult (type Declared(CTsrStructuredCellsResult) is not supported)
pdf_inspector.h:193:16: warning: Skipping CTsrTableExtractionResult (type Declared(CTsrTableExtractionResult) is not supported)
pdf_inspector.h:200:16: warning: Skipping CVectorGridResult (type Declared(CVectorGridResult) is not supported)
pdf_inspector.h:213:16: warning: Skipping PdfOptions (type Declared(PdfOptions) is not supported)
__stddef_max_align_t.h:22:15: warning: Skipping __clang_max_align_nonce2 (type LongDouble is not supported)
```

The twelve `C*Result`/`PdfOptions` warnings are jextract declining to generate a _struct class_ for each opaque handle typedef; it has no known fields to model (that's the point of an opaque handle). It still generates every function that takes or returns a pointer to one of these types, typed as `MemorySegment`, which is exactly the right Java representation: callers pass handles around and never dereference them. Do not try to "fix" these warnings; there is nothing to fix. The `__clang_max_align_nonce2` warning comes from a system header pulled in incidentally and disappears once the filter in §2 restricts generation to `pdf_inspector.h`'s own symbols.

## 6. FFM usage idioms

### Borrowed UTF-8 `CByteView` strings

jextract represents `CByteView` as a generated struct with `ptr` and `len` accessors. The pointer is borrowed, **not** NUL-terminated, and extracted PDF text can legitimately contain interior NUL bytes (this matches [`docs/c-api.md`](c-api.md#ownership-and-safety)), so `MemorySegment.getString()` and anything based on `strlen` give wrong answers; read the view's length and slice the pointer explicitly:

```java
static String readUtf8(MemorySegment view) {
    MemorySegment ptr = CByteView.ptr(view);
    long len = CByteView.len(view);
    if (ptr.equals(MemorySegment.NULL) || len == 0) return "";
    return new String(ptr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
}
```

### `T**` out-parameters

```java
MemorySegment resultOut = arena.allocate(ValueLayout.ADDRESS);   // T** slot
int rc = pdf_inspector_h.pdf_inspector_process_pdf(cPath, options, resultOut);
MemorySegment result = resultOut.get(ValueLayout.ADDRESS, 0);    // the T* it received
```

### Borrowed `CU32View` arrays

Array getters such as `pdf_inspector_result_get_pages_with_tables(result, CU32View *out)` write a borrowed pointer/length view:

```java
MemorySegment view = CU32View.allocate(arena);
if (!pdf_inspector_h.pdf_inspector_result_get_pages_with_tables(result, view)) {
    throw new IllegalStateException("pages-with-tables view unavailable");
}
long n = CU32View.len(view);
int[] pages = n == 0 ? new int[0]
        : CU32View.ptr(view).reinterpret(n * ValueLayout.JAVA_INT.byteSize())
                .toArray(ValueLayout.JAVA_INT);
```

### Handle ownership

Map the C API's ownership rules directly: a handle is caller-owned and must be freed exactly once, and a `MemorySegment` read from a borrowed `CByteView`/`CU32View` must not outlive the handle it came from. Wrap each handle type in a small `AutoCloseable`:

```java
public final class OptionsHandle implements AutoCloseable {
    private MemorySegment segment = pdf_inspector_h.pdf_inspector_options_new();
    public MemorySegment segment() { return segment; }
    @Override public void close() {
        if (segment != null) { pdf_inspector_h.pdf_inspector_options_free(segment); segment = null; }
    }
}
```

Use `Arena.ofConfined()` for the call-scoped inputs (path strings, out-param slots) around each unit of work, and free that arena's memory deterministically when the `try`-with-resources block exits. `Arena.ofAuto()` (backed by `Cleaner`/GC) is the wrong owner for these _handles_ specifically: a `*_free` call must happen exactly once at a point you control, and GC-triggered cleanup gives you neither "exactly once" (a `Cleaner` can in principle run more than once if you mismanage the cleanable) nor "at a point you control" (freeing an options handle while another thread still holds a `MemorySegment` sliced from one of its results is a use-after-free, and GC timing can't guarantee that doesn't happen). Free handles explicitly.

### Threading

The C ABI documents that concurrent reads of one handle are safe, and any call that mutates or frees a handle must not race with another use of it ([`docs/c-api.md`](c-api.md#ownership-and-safety)). Reproduced from Java: 16 threads × 12 iterations, each computing a checksum over every field of a shared 1699-item `CTextItemsResult` handle concurrently; 0 mismatches against a single-threaded reference checksum, no exceptions. (`examples/java` does not ship this test; it mirrors the pattern above using `Thread`/`AtomicLong`.)

### Error handling

Check the returned `int` against the `PdfInspectorError_*` constants (see [Enum constants](#enum-constants) below). The ABI also exposes `pdf_inspector_last_error_message(CByteView *out)`, which writes a borrowed view for the diagnostic behind the most recent fallible call on the calling thread (see [`docs/c-api.md`](c-api.md#error-diagnostics) for the full scoping rules). It is **not** populated for every error: a nonexistent path returns `PdfInspectorError_IoError` with an empty view, while a plain-text file passed as the PDF returns `PdfInspectorError_NotAPdf` with the message `"file appears to be plain text"`. Per `docs/c-api.md`, only `ParseError` and `NotAPdf` currently carry extra text; every other code is empty there. So always check the boolean and view pointer before use, and don't assume a non-`Success` return means a non-empty message:

```java
int rc = pdf_inspector_h.pdf_inspector_process_pdf(cPath, options, resultOut);
if (rc != pdf_inspector_h.PdfInspectorError_Success()) {
    MemorySegment errorView = CByteView.allocate(arena);
    boolean hasDetail = pdf_inspector_h.pdf_inspector_last_error_message(errorView);
    String detail = hasDetail ? readUtf8(errorView) : "";
    throw new RuntimeException("pdf_inspector error " + rc + (detail.isEmpty() ? "" : ": " + detail));
}
```

### Enum constants

jextract turns C enum constants into `static int` accessor methods, not Java enums. Call them, don't try to reference a field:

```java
pdf_inspector_h.pdf_inspector_options_set_mode(options, pdf_inspector_h.CProcessMode_Full());
if (rc == pdf_inspector_h.PdfInspectorError_Success()) { ... }
```

If you want a real Java `enum` at your API boundary, map it once at the edge (a `switch` from `int` to your enum) rather than passing raw `int`s through application code.

## 7. Performance, measured

The ABI is a per-field getter design: reading one positioned text item's 16 fields costs 16 downcalls. Measured on `tests/fixtures/2013-app2.pdf` (8 pages, 1699 text items → 1699 × 16 = 27184 downcalls), via `examples/java`'s loader against the filtered bindings, JIT warmed with repeated sweeps over the same handle:

| Phase | Cost |
| --- | --- |
| First sweep after linkage (each of the 16 accessor `MethodHandle`s bound for the first time) | ~13 ms (~480 ns/downcall) |
| Sweep #10, interpreter/C1 settled | ~0.70 ms (~26 ns/downcall) |
| Steady-state (avg of 100 sweeps, C2-compiled) | ~0.29 ms (~11 ns/downcall) |
| Full materialization incl. UTF-8 `String` decode, steady-state | ~530-590 ns/item |
| `process_pdf` full extraction, this-process first call | ~44-48 ms |
| `process_pdf` full extraction, steady-state (avg of 10) | ~38-39 ms |

So the steady-state field sweep (~0.3 ms) is **~0.8%** of steady-state extraction (~39 ms), and full materialization of every item's text and font strings (~1699 × 560 ns ≈ 0.95 ms) is **~2.4%**. Even the one-time linkage cost (~13 ms, paid once per JVM process the first time these particular accessors are called) is a fraction of a single extraction call. **The FFM downcall overhead is not a bottleneck; UTF-8 `String` decoding dominates** the per-item cost once warm (530 ns/item decoding two strings vs. ~11 ns per individual field downcall). Practical advice: read only the fields a caller actually needs, and decode a `CByteView` only when needed; a decode you skip is free.

These are single-machine numbers with expected run-to-run variance (±10-15% observed across repeated runs); the _shape_ of the result, downcalls negligible, decoding dominant, both negligible next to extraction itself, is the point, not the exact nanosecond figures.

## 8. Complete example

[`examples/java/`](../examples/java/) contains a full, runnable example:

- `filter.txt`: the `--dump-includes` filter from [§2](#2-trimming-the-generated-surface)
- `PdfInspectorLoader.java`: the loader from [§3](#3-loading-and-packaging)
- `Main.java`: `process_pdf` → read Markdown through `CByteView` → read a `CU32View` array → positioned text items, exercising every idiom in [§6](#6-ffm-usage-idioms)
- `build.sh`: jextract → `javac` → run, in one script

```bash
$ ./examples/java/build.sh
Markdown: 30062 chars, 8 pages
Pages with tables: [1, 2, 3, 4, 5, 6, 8]
Text items: 1699
Item 0: "Date" at (46.32, 547.08)
```

That is real, unedited output from running the script against `tests/fixtures/2013-app2.pdf` on this machine (plus the twelve expected opaque-handle jextract warnings from §5, omitted above).

## 9. CI recipe

No CI job wires this up yet; the project's CI only builds and tests the Rust/C layers. `scripts/check-java-filter.py` is the one piece that does run today, and only locally: it checks that `examples/java/filter.txt` covers every symbol `pdf_inspector.h` exports, so a newly added C function can't silently drop out of the Java bindings without at least a local signal. Run it after regenerating the header:

```bash
./scripts/generate-c-header.sh
python3 scripts/check-java-filter.py
```

Wiring up the rest, regenerating bindings, compiling them, and running a smoke test, so a header change that breaks the Java layer is caught in CI rather than discovered by a downstream consumer, is left to whoever adds it. The Linux recipe:

```yaml
# Linux job
- run: cargo build --release --lib --features c-api
- run: jextract --output build/java-src -t com.pdfinspector.ffi @examples/java/filter.txt pdf_inspector.h
- run: cp examples/java/Main.java examples/java/PdfInspectorLoader.java build/java-src/
- run: javac -d build/java-classes $(find build/java-src -name '*.java')
- run: mkdir -p build/native && cp target/release/libpdf_inspector.so build/native/
- run: |
    java --enable-native-access=ALL-UNNAMED \
         -cp build/java-classes:build \
         Main tests/fixtures/2013-app2.pdf
    # assert exit 0 and non-empty Markdown output
```

This is exactly `examples/java/build.sh`'s four steps, one per CI `run:` line. See that script for a version that actually runs.

Such a job should fail on:

- non-zero jextract exit, or a warning count other than the 12 expected opaque-handle warnings from §5 (a new opaque type or an unrelated system-header pull-in both show up as new warning lines)
- `javac` failure (a signature change jextract can't represent the same way, or a type that stops matching the `Idioms`/`PdfInspectorLoader` helpers' assumptions)
- the smoke test failing to load the library or returning a non-success error code

This would mirror `./scripts/generate-c-header.sh` and `./scripts/test-c-consumer.sh`'s role for the C consumer in CI, with the same idea one layer up.
