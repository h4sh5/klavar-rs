#!/usr/bin/env bash
# Assemble the deployable frontend-only app into docs/ (GitHub Pages).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

cargo build --release            # native sanity check
wasm-pack build --target web --release --out-dir pkg

rm -rf docs
mkdir -p docs
cp web/index.html web/style.css web/app.js docs/
cp -r pkg docs/pkg
cp -r web/samples docs/samples
# wasm-pack's .gitignore inside pkg/ ignores itself; clean up
rm -f docs/pkg/.gitignore

echo "docs/ ready ($(du -sh docs | cut -f1)). Serve with: python3 -m http.server -d docs 8080"
