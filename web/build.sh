#!/usr/bin/env bash
# Builds Horizon for the browser into web/dist: patched std (web/std), wasm threads, WebGL2.
# Usage: web/build.sh [check|debug|release]  (default release)
set -euo pipefail
cd "$(dirname "$0")/.."
MODE=${1:-release}
python3 web/std/apply.py >/dev/null
export RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals,+nontrapping-fptoint,+sign-ext \
 -C link-arg=--shared-memory -C link-arg=--max-memory=4294967296 -C link-arg=--import-memory \
 -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size -C link-arg=--export=__tls_align \
 -C link-arg=--export=__tls_base -C link-arg=-zstack-size=4194304 --cfg getrandom_backend=\"wasm_js\""
ARGS=(--target wasm32-unknown-unknown -Zbuild-std=std,panic_abort -p horizon-web --target-dir target/web)
case "$MODE" in
  check) exec cargo +nightly check "${ARGS[@]}" ;;
  debug) cargo +nightly build "${ARGS[@]}"; PROFILE=debug ;;
  release) cargo +nightly build --profile web-release "${ARGS[@]}"; PROFILE=web-release ;;
esac
OUT=web/dist
rm -rf "$OUT"; mkdir -p "$OUT"
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir "$OUT" "target/web/wasm32-unknown-unknown/$PROFILE/horizon_web.wasm"
python3 web/runtime/fix_glue.py "$OUT/horizon_web.js"
cp web/runtime/hzweb.js web/runtime/hzweb_worker.js web/launcher/* "$OUT/" 2>/dev/null || true
ls -la "$OUT"
