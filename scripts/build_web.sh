#!/usr/bin/env bash
# Assemble the deployable frontend-only app into web-app/.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

cargo build --release            # native sanity check
wasm-pack build --target web --release --out-dir pkg

rm -rf web-app
mkdir -p web-app
cp web/index.html web/style.css web/app.js web-app/
cp -r pkg web-app/pkg
cp -r web/samples web-app/samples
# wasm-pack's .gitignore inside pkg/ ignores itself; clean up
rm -f web-app/pkg/.gitignore

echo "web-app/ ready ($(du -sh web-app | cut -f1)). Serve with: python3 -m http.server -d web-app 8080"
