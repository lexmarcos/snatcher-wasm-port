#!/usr/bin/env bash
# Build the wasm engine and the web front end into web/pkg and web/dist.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p snengine --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/snengine.wasm
cd web && bun run build
