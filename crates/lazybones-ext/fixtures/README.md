# lazybones-ext test fixtures

## `wasm/gate-check-example.wasm`

A prebuilt WASI Preview 2 **component** implementing the `lazybones:ext/gate-check`
interface (`../wit/world.wit`). The host tests in `../tests/gate_check.rs` load it
to prove the runtime end-to-end: typed input/output over `call_async`, fuel/epoch/
memory limits, and trap isolation.

It is **checked in** so `cargo test -p lazybones-ext` needs no wasm toolchain. Its
source lives in `examples/gate-check-example/` (a standalone crate, excluded from
the workspace).

### Rebuilding

Requires the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`):

```sh
./build.sh
```

which is just:

```sh
cd examples/gate-check-example
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/gate_check_example.wasm ../../wasm/gate-check-example.wasm
```

The `wasm32-wasip2` target emits a Component Model component directly — no separate
`cargo component` / `wasm-tools` step is needed.

### Guest verdict policy

- empty diff (`files-changed == 0`) → `skip`
- `task-summary` contains `"fail"` → `fail`
- otherwise → `pass`
- `task-summary == "runaway"` → spins forever (so the host can assert the
  fuel/epoch limiter kills it)
