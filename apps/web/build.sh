#!/bin/sh
# Build the browser client's WebAssembly module.
#
# No bundler and no npm. The page is one HTML file that imports the module
# wasm-pack generates, which keeps "no install, no signing" true for the person
# hosting it as well as for the person using it.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out="$root/apps/web/pkg"

if ! command -v wasm-pack >/dev/null 2>&1; then
    echo "wasm-pack is not installed. Install it with:" >&2
    echo "    cargo install wasm-pack" >&2
    exit 1
fi

wasm-pack build "$root/crates/wow-coach-wasm" \
    --target web \
    --out-dir "$out" \
    --out-name wow_coach_wasm \
    --no-typescript \
    "$@"

echo
echo "Built into apps/web/pkg. Serve the folder — a module will not load from file://:"
echo "    python3 -m http.server --directory apps/web 8080"
