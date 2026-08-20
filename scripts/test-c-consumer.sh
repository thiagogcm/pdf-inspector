#!/usr/bin/env sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"

./scripts/generate-c-header.sh
# The C symbols live behind the `c-api` feature; without it the archive links
# but exports nothing.
cargo build --release --lib --features c-api

lib_dir="target/release"

# `cargo build` only produces the unversioned libpdf_inspector.so/.dylib, but
# its own SONAME/install name is versioned (see build.rs and docs/c-api.md's
# "Building" section) -- symlink the versioned name here so this repo's own
# build output actually links and runs.
versioned_lib=""
os=$(uname -s 2>/dev/null || echo unknown)
if [ "$os" = "Linux" ] && command -v readelf >/dev/null 2>&1 && [ -f "$lib_dir/libpdf_inspector.so" ]; then
  soname=$(readelf -d "$lib_dir/libpdf_inspector.so" 2>/dev/null | sed -n 's/.*(SONAME)[^[]*\[\(.*\)\]/\1/p')
  if [ -n "$soname" ] && [ "$soname" != "libpdf_inspector.so" ]; then
    ln -sf libpdf_inspector.so "$lib_dir/$soname"
    versioned_lib="$lib_dir/$soname"
  fi
elif [ "$os" = "Darwin" ] && command -v otool >/dev/null 2>&1 && [ -f "$lib_dir/libpdf_inspector.dylib" ]; then
  # Best-effort; unlike Linux, not exercised by CI.
  install_name=$(otool -D "$lib_dir/libpdf_inspector.dylib" 2>/dev/null | tail -n 1)
  versioned_name=$(basename "$install_name" 2>/dev/null || true)
  if [ -n "$versioned_name" ] && [ "$versioned_name" != "libpdf_inspector.dylib" ]; then
    ln -sf libpdf_inspector.dylib "$lib_dir/$versioned_name"
  fi
fi

consumer_binary=$(mktemp "${TMPDIR:-/tmp}/pdf-inspector-c-consumer.XXXXXX")
short_enums_binary=$(mktemp "${TMPDIR:-/tmp}/pdf-inspector-c-consumer-short-enums.XXXXXX")
dynamic_binary=$(mktemp "${TMPDIR:-/tmp}/pdf-inspector-c-consumer-dynamic.XXXXXX")
trap 'rm -f "$consumer_binary" "$short_enums_binary" "$dynamic_binary"' EXIT
${CC:-cc} -std=c11 -Wall -Wextra -Werror -I. tests/c_consumer.c target/release/libpdf_inspector.a -ldl -lpthread -lm -o "$consumer_binary"
${CXX:-c++} -std=c++17 -x c++ -fsyntax-only -I. -include pdf_inspector.h /dev/null
"$consumer_binary"

# Recompile and rerun under `-fshort-enums` (see the `_Static_assert`s in
# tests/c_consumer.c) so an enum-width regression fails loudly here.
${CC:-cc} -std=c11 -Wall -Wextra -Werror -fshort-enums -I. tests/c_consumer.c target/release/libpdf_inspector.a -ldl -lpthread -lm -o "$short_enums_binary"
"$short_enums_binary"

# Dynamic-link smoke test (`-lpdf_inspector`, not the static archive above).
# Linux-only: ELF-specific, and the only platform the c-abi CI job runs on.
if [ "$os" = "Linux" ] && [ -n "$versioned_lib" ]; then
  ${CC:-cc} -std=c11 -Wall -Wextra -Werror -I. tests/c_consumer.c -L"$lib_dir" -lpdf_inspector -ldl -lpthread -lm -o "$dynamic_binary"
  LD_LIBRARY_PATH="$lib_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" "$dynamic_binary"
fi
