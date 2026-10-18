#!/usr/bin/env bash
# Build the wasm engine and the web front end into web/pkg and web/dist.
set -euo pipefail
cd "$(dirname "$0")/.."
# translation pack (pt-BR) for the engine; reads only i18n/pt-BR/*.json, no game data
cargo build --release -j 4 -p sntools
./target/release/snt i18n-build i18n/pt-BR i18n/build/pt-BR.pack
cargo build -j 4 -p snengine --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/snengine.wasm
cd web && bun run build
