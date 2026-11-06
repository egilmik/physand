#!/usr/bin/env bash
# Build the Rust simulation to wasm and generate JS bindings into web/src/wasm.
# Usage: scripts/build-wasm.sh [--dev]
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE=release
FLAGS=(--release)
if [[ "${1:-}" == "--dev" ]]; then
  PROFILE=debug
  FLAGS=()
fi

cargo build -p physand-web --target wasm32-unknown-unknown "${FLAGS[@]}"
wasm-bindgen \
  --target web \
  --out-dir web/src/wasm \
  --out-name physand \
  "target/wasm32-unknown-unknown/${PROFILE}/physand_web.wasm"
echo "wasm built ($PROFILE) -> web/src/wasm"
