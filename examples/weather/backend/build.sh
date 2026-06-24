#!/bin/sh
# Build the `weather` backend guest and stamp it with the embedded
# `lazybones.ext.toml` manifest, producing the installable `dist/weather.wasm`.
#
# Requires:  rustup target add wasm32-wasip2   (and python3 for the embed step)
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/.." && pwd)"

cd "$here"
cargo build --release --target wasm32-wasip2

raw="target/wasm32-wasip2/release/weather.wasm"
out="$root/dist/weather.wasm"

mkdir -p "$root/dist"
python3 "$here/embed-manifest.py" "$raw" "$root/lazybones.ext.toml" "$out"

echo "installable component: $out"
