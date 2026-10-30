#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --locked --release -p rcode-wasm --target wasm32-unknown-unknown
mkdir -p dist/web/pkg
wasm-bindgen target/wasm32-unknown-unknown/release/rcode_wasm.wasm \
  --target web --out-dir dist/web/pkg --no-typescript
cp web/app.js web/style.css web/static-api.js web/wasm-worker.js dist/web/
python3 - <<'PY'
from pathlib import Path

html = Path("web/index.html").read_text()
html = html.replace('href="/style.css"', 'href="./style.css"')
html = html.replace('<script src="/app.js" defer></script>',
                    '<script src="./static-api.js" defer></script>\n'
                    '    <script src="./app.js" defer></script>')
html = html.replace("内容只发送到当前本机服务，不经第三方。", "内容与图片仅在当前浏览器内处理，不上传服务器。")
Path("dist/web/index.html").write_text(html)
Path("dist/web/.nojekyll").touch()
PY
