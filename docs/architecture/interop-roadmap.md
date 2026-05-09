# Polyglot interop roadmap (Rust front, Swift)

This doc ties optional embedding work to concrete crates and repo paths. Nothing here is required for `cargo install inauguration`; it guides **in-tree** consolidation.

## Boundaries today

| Piece | Role | Today |
|-------|------|-------|
| Swift subset front | Parse/check minimal Swift-ish lines → JSON artifact; optional **SIL** for `in build` | **`in ocaml`** → `in-cli/src/swift_subset.rs`; **`IN_NATIVE_SWIFT_SIL`** → `in-cli/src/native_swift_sil.rs` (no `swiftc` when subset matches) |
| Protocol enums (`PatchType`) | Rust + Swift from JSON Schema | **Rust `protocol-gen`** (`cargo run --manifest-path in-cli/Cargo.toml --bin protocol-gen`) is canonical checked-in codegen; CI uses **`scripts/check-protocol-models.sh`**. **V** **`shared/protocol/generate_models.v`** is optional minor-tool parity (same outputs as `protocol-gen` aside from header comment). |
| Benchmark harness | Swift vs `in` timings + markdown/json reports | **`./scripts/bench-swift.sh`** runs **`v -gc none run $ROOT/scripts/bench_swift.v`** (sets **`BENCH_ROOT`**). |
| Preview pipeline | NDJSON over Unix socket | **`in dev`** defaults to **Rust** client; **`--preview-client swift`** runs SwiftPM **`swift-preview-host-client`** |

## swift-rs (macOS Swift linkage)

Add **`swift-rs`** + **`build-dependencies swift-rs` with `features = ["build"]`**, then **`SwiftLinker`** in `build.rs` against an SPM **static** library with **`@_cdecl`** exports (see swift-rs README). Until then, **`in dev --preview-client swift`** shells **`swift run`** for the Swift bridge.

## Suggested phases

1. **Phase A — Contract freeze:** JSON schema + frontend artifact JSON shape stay the cross-language contracts.
2. **Phase B — Richer Rust front:** extend `swift_subset` parsing beyond line declarations without breaking `format_version`.
3. **Phase C — Preview bridge:** SPM static lib + **swift-rs** for **`PreviewHost`** without subprocess.

## Related paths

- Phased roadmap (subset AST → SIL → hotreload): [`native-swift-master-plan.md`](native-swift-master-plan.md).
- Parser/check entry: `in-cli/src/swift_subset.rs`.
- Socket client reference: `runtime/swift-preview-host/Sources/SwiftPreviewHostClient/main.swift`.
