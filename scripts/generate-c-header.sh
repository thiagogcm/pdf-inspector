#!/usr/bin/env sh
set -eu

CBINDGEN_VERSION=0.29.4

if ! command -v cbindgen >/dev/null 2>&1; then
  echo "cbindgen ${CBINDGEN_VERSION} is required; install it with: cargo install cbindgen --version ${CBINDGEN_VERSION} --locked" >&2
  exit 1
fi

if [ "$(cbindgen --version)" != "cbindgen ${CBINDGEN_VERSION}" ]; then
  echo "cbindgen ${CBINDGEN_VERSION} is required; found: $(cbindgen --version)" >&2
  exit 1
fi

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
cbindgen --quiet --config cbindgen.toml --crate pdf-inspector --output pdf_inspector.h
