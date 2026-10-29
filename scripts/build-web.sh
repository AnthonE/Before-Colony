#!/usr/bin/env bash
# Builds the browser client into web/dist/<variant>/ (variants: webgl2, webgpu).
#   scripts/build-web.sh            # webgl2 only (fast path)
#   scripts/build-web.sh webgl2 webgpu
#   scripts/build-web.sh page       # only the page and its gallery (no wasm): then open /?gallery
# Env: BC_WEB_PROFILE (default wasm-release), BC_WEB_OPT=1 to run wasm-opt when available;
# CARGO_TARGET_DIR is followed.
set -euo pipefail
cd "$(dirname "$0")/.."

variants=("$@")
[ ${#variants[@]} -eq 0 ] && variants=(webgl2)
profile="${BC_WEB_PROFILE:-wasm-release}"

# wasm-bindgen's CLI must match the crate version exactly (only the page: no wasm to bind).
if [ "${variants[*]}" != page ]; then
  crate_ver=$(awk '/^name = "wasm-bindgen"$/{getline; print $3}' Cargo.lock | tr -d '"' | head -1)
  cli_ver=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
  if [ -z "$cli_ver" ]; then
    echo "wasm-bindgen CLI not found: cargo install wasm-bindgen-cli --version ${crate_ver} --locked" >&2
    exit 1
  fi
  if [ "$crate_ver" != "$cli_ver" ]; then
    echo "wasm-bindgen CLI ${cli_ver} != crate ${crate_ver}" >&2
    exit 1
  fi
fi

mkdir -p web/dist
cp web/index.html web/config.js web/boot.js web/loader.js web/ui.js web/floor.js web/gallery.js web/style.css web/favicon.svg web/dist/
mkdir -p web/dist/fonts && cp web/fonts/* web/dist/fonts/
# The store's traders' avatars (bc_client_core::avatar), fetched when one is in view.
mkdir -p web/dist/assets && cp -r web/assets/. web/dist/assets/
# The page's gallery (`?gallery`): every screen with made-up data (bc_client_core::gallery).
cargo run -q -p bc-client-core --example page_gallery > web/dist/gallery.json

for v in "${variants[@]}"; do
  [ "$v" = page ] && continue
  feats=()
  case "$v" in
    webgl2) ;;
    webgpu) feats=(--features webgpu) ;;
    *) echo "unknown variant $v" >&2; exit 1 ;;
  esac
  echo "==> building $v ($profile)"
  cargo build -p bc-client --target wasm32-unknown-unknown --profile "$profile" "${feats[@]}"
  out="web/dist/$v"
  rm -rf "$out"
  wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name bc \
    "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/$profile/bc-client.wasm"
  if [ "${BC_WEB_OPT:-0}" = "1" ] && command -v wasm-opt >/dev/null; then
    echo "==> wasm-opt -Oz $v"
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int -o "$out/bc_bg.wasm" "$out/bc_bg.wasm"
  fi
  # Precompressed copies; the dev server serves them when the browser accepts br/gzip.
  node -e '
    const fs = require("fs"), z = require("zlib");
    for (const f of process.argv.slice(1)) {
      const raw = fs.readFileSync(f);
      fs.writeFileSync(f + ".br", z.brotliCompressSync(raw, { params: { [z.constants.BROTLI_PARAM_QUALITY]: 9 } }));
      fs.writeFileSync(f + ".gz", z.gzipSync(raw, { level: 9 }));
    }' "$out/bc_bg.wasm" "$out/bc.js"
  raw=$(stat -c %s "$out/bc_bg.wasm"); br=$(stat -c %s "$out/bc_bg.wasm.br")
  echo "==> $v: wasm $((raw / 1024 / 1024)) MiB raw, $((br / 1024 / 1024)) MiB brotli"
done
