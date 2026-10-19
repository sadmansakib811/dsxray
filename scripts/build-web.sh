#!/bin/sh
# Build the browser page into site/: the static files from web/, the WebAssembly parser and a sample file.
#   scripts/build-web.sh && python3 -m http.server -d site 8765   (then open http://127.0.0.1:8765)
set -eu
cd "$(dirname "$0")/.."
cargo build -p dsxray-wasm --target wasm32-unknown-unknown --profile wasm
rm -rf site
mkdir site
cp web/index.html web/style.css web/app.js site/
cp target/wasm32-unknown-unknown/wasm/dsxray_wasm.wasm site/dsxray.wasm
cp crates/dsxray/tests/fixtures/small.DS_Store site/sample.DS_Store
echo "site/ ready: $(du -sh site | cut -f1)"
