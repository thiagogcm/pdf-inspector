#!/usr/bin/env sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
./scripts/generate-c-header.sh
cargo build --release --lib
work=$(mktemp -d "${TMPDIR:-/tmp}/pdf-inspector-c.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

# Compare the C compiler's complete record layouts with Rust's layouts.
cargo test --release --lib tests::emit_c_layout_contract -- --nocapture > "$work/layout.log"
sed -n 's/^PDF_LAYOUT: //p' "$work/layout.log" > "$work/layout.h"
test -s "$work/layout.h"
CARGO_TERM_COLOR=never cargo rustc --release --lib -- --print native-static-libs 2> "$work/native-libs.log"
native_libs=$(sed -n 's/.*native-static-libs: //p' "$work/native-libs.log")
test -n "$native_libs"
cc=${CC:-cc}
cxx=${CXX:-c++}
os=$(uname -s)
case "$os" in
    MINGW*|MSYS*|CYGWIN*)
        cc=${CC:-clang}
        cxx=${CXX:-clang++}
        "$cc" -std=c11 -Wall -Wextra -Werror -I. -include pdf_inspector.h -include "$work/layout.h" tests/c_consumer.c target/release/pdf_inspector_c.lib $native_libs -o "$work/static.exe"
        "$work/static.exe"
        "$cc" -std=c11 -Wall -Wextra -Werror -I. -include pdf_inspector.h -include "$work/layout.h" tests/c_consumer.c target/release/pdf_inspector_c.dll.lib -o "$work/consumer.exe"
        PATH="$PWD/target/release:$PATH" "$work/consumer.exe"
        ;;
    *)
        # Word splitting is intentional for the platform's native link libraries.
        "$cc" -std=c11 -Wall -Wextra -Werror -I. -include pdf_inspector.h -include "$work/layout.h" tests/c_consumer.c target/release/libpdf_inspector_c.a $native_libs -o "$work/static"
        "$work/static"
        "$cc" -std=c11 -Wall -Wextra -Werror -fshort-enums -I. tests/c_consumer.c target/release/libpdf_inspector_c.a $native_libs -o "$work/short-enums"
        "$work/short-enums"
        "$cc" -std=c11 -Wall -Wextra -Werror -I. tests/c_consumer.c -Ltarget/release -lpdf_inspector_c -Wl,-rpath,"$PWD/target/release" -o "$work/dynamic"
        "$work/dynamic"
        ;;
esac
"$cxx" -std=c++17 -x c++ -fsyntax-only -I. -include pdf_inspector.h /dev/null
