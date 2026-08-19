#!/usr/bin/env bash
# Regenerate Java FFM bindings for pdf_inspector.h, compile the example, and
# run it against a fixture. Run from the repo root.
#
# Requires: JDK >= 22 with jextract on PATH (or set JEXTRACT below), and
# target/release/libpdf_inspector.* already built with:
#   cargo build --release --lib --features c-api
set -euo pipefail

JEXTRACT="${JEXTRACT:-jextract}"
OUT=target/java-bindings
PKG=com.pdfinspector.ffi
native_dir="$OUT/native"
library_path="target/release/libpdf_inspector.so"

rm -rf "$OUT"
mkdir -p "$OUT/src" "$OUT/classes" "$native_dir"

# 1. Filter out everything jextract pulled in from system/libc headers, keeping
#    only pdf_inspector.h's own symbols (see examples/java/filter.txt for how
#    filter.txt itself is derived from --dump-includes).
"$JEXTRACT" --output "$OUT/src" -t "$PKG" @examples/java/filter.txt pdf_inspector.h

# 2. Compile bindings + example. No --library was passed above, so the
#    generated code uses loaderLookup().or(defaultLookup()); see docs/java-bindings.md.
cp examples/java/Main.java examples/java/PdfInspectorLoader.java "$OUT/src/"
javac -d "$OUT/classes" $(find "$OUT/src" -name '*.java')

# 3. Stage the Linux native library where PdfInspectorLoader expects it.
cp "$library_path" "$native_dir/"

# 4. Run. --enable-native-access silences the JEP 472 restricted-method
#    warnings from System.load and the generated AddressLayout usage.
java --enable-native-access=ALL-UNNAMED \
     -cp "$OUT/classes:$OUT" \
     Main tests/fixtures/2013-app2.pdf
