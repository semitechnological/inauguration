# Polyglot interop roadmap (OCaml, Swift)

This doc ties optional embedding work to concrete crates and repo paths. Nothing here is required for `cargo install inauguration`; it guides **in-tree** consolidation.

## Boundaries today

| Piece | Role | Today |
|-------|------|--------|
| OCaml front | Parse/check Swift subset → JSON artifact | STDIN/STDOUT CLI (`compiler/ocaml-front/bin/main.ml`) + **`in ocaml`** |
| Protocol enums (`PatchType`) | Rust + Swift from JSON Schema | **`cargo run --manifest-path in-cli/Cargo.toml --bin protocol-gen`** (`serde_json` on `events.schema.json`) |
| Preview pipeline | NDJSON over Unix socket | **`in dev`** defaults to **Rust** client; **`--preview-client swift`** runs SwiftPM **`swift-preview-host-client`** |

## OCaml ↔ Rust

Build with **`cargo build --manifest-path in-cli/Cargo.toml --features experimental-ocaml-interop`** only when OCaml 5.x / opam are available (otherwise `ocaml-sys` build scripts fail). Default crates.io builds omit this feature.

| Crate | Best when | Notes |
|-------|-----------|--------|
| **ocaml-interop** | Rust **hosts** OCaml | GC rooting via Rust types |
| **ocaml-rs** (+ macros) | OCaml hosts Rust | `#[ocaml::func]` |

**Concrete OCaml surface to FFI-wrap:** parse string → check → `Artifact.program_to_json` → JSON string (mirror `bin/main.ml`).

## swift-rs (macOS Swift linkage)

Add **`swift-rs`** + **`build-dependencies swift-rs` with `features = ["build"]`**, then **`SwiftLinker`** in `build.rs` against an SPM **static** library with **`@_cdecl`** exports (see swift-rs README). Until then, **`in dev --preview-client swift`** shells **`swift run`** for the Swift bridge.

## Suggested phases

1. **Phase A — Contract freeze:** JSON schema + OCaml artifact JSON shape stay the cross-language contracts.
2. **Phase B — OCaml embed:** Link OCaml runtime + wrap `program_to_json` via **ocaml-interop**, with **`experimental-ocaml-interop`** feature.
3. **Phase C — Preview bridge:** SPM static lib + **swift-rs** for **`PreviewHost`** without subprocess.

## Related paths

- OCaml front entry: `compiler/ocaml-front/bin/main.ml`.
- Socket client reference: `runtime/swift-preview-host/Sources/SwiftPreviewHostClient/main.swift`.
