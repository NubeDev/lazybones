#!/usr/bin/env python3
"""Embed `lazybones.ext.toml` into a wasm component as a custom section.

The lazybones host reads an extension's declared identity/caps/extension-points
from a **custom section** named `lazybones.ext.toml` carried by the component
(see `crates/lazybones-ext/src/manifest.rs::extract_embedded`). The
`wasm32-wasip2` build emits the component WITHOUT that section, so this script
appends it.

A wasm custom section is `0x00 <uleb section_len> <uleb name_len> <name> <data>`
and is valid at the top level of both core modules and components, so appending
one to the end of the component bytes is sufficient for the host's scanner.

Usage:  embed-manifest.py <input.wasm> <manifest.toml> <output.wasm>
"""
import sys

SECTION_NAME = b"lazybones.ext.toml"


def uleb(n: int) -> bytes:
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def main() -> int:
    if len(sys.argv) != 4:
        sys.stderr.write(
            "usage: embed-manifest.py <input.wasm> <manifest.toml> <output.wasm>\n"
        )
        return 2
    in_wasm, manifest, out_wasm = sys.argv[1:4]

    with open(in_wasm, "rb") as f:
        wasm = f.read()
    with open(manifest, "rb") as f:
        data = f.read()

    body = uleb(len(SECTION_NAME)) + SECTION_NAME + data
    section = b"\x00" + uleb(len(body)) + body

    with open(out_wasm, "wb") as f:
        f.write(wasm + section)

    print(f"wrote {out_wasm} ({len(wasm) + len(section)} bytes, "
          f"manifest section {len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
