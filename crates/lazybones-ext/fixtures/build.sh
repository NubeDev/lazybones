#!/bin/sh
# Rebuild the checked-in gate-check example component from source.
# Requires: rustup target add wasm32-wasip2
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/examples/gate-check-example"

cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/gate_check_example.wasm \
   "$here/wasm/gate-check-example.wasm"

echo "wrote $here/wasm/gate-check-example.wasm"
