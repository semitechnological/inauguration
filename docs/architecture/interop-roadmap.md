# Polyglot interop roadmap (OCaml, V/Equilibrium, Swift)

This doc ties optional embedding work to concrete crates and repo paths. Nothing here is required for `cargo install inauguration`; it guides **in-tree** consolidation.

## Boundaries today

| Piece | Role | Today |
|-------|------|--------|
| OCaml front | Parse/check Swift subset → JSON artifact | STDIN/STDOUT CLI (`compiler/ocaml-front/bin/main.ml`) |
| V codegen | `PatchType` / Swift enum from JSON Schema | `shared/protocol/generate_models.v` + `v -gc none run …` |
| Preview pipeline | NDJSON envelopes over Unix socket → apply patches | Swift `swift-preview-host-client` + `PreviewHost` actor |

## OCaml ↔ Rust

**Goal:** Either keep OCaml as a **library callable from Rust** or keep **subprocess** (simplest, GC isolation).

| Crate | Best when | Notes |
|-------|-----------|--------|
| **ocaml-interop** | Rust **owns the runtime** and must hold OCaml values safely | GC **rooting** is explicit in Rust’s type system; good fit for **embedding OCaml inside `in`** long-term |
| **ocaml-rs** (+ macros) | Writing **native OCaml stubs** that call Rust | Ergonomic `#[ocaml::func]` when OCaml is the host |
| **derive-ocaml** | Less boilerplate on top of ocaml-rs | Use if ocaml-rs wins |
| **ocaml-gen** | Generate OCaml signatures from Rust | Useful if Rust becomes source of truth for shared types — **less relevant** while OCaml owns the AST |

**Recommendation:** If `in` embeds the frontend, prefer **ocaml-interop** for safety around long-lived OCaml values from Rust. Until then, **subprocess + JSON** stays the contract (already matches `main.ml`).

**Concrete OCaml surface to FFI-wrap:** one function equivalent to: parse string → check → `Artifact.program_to_json` → JSON string (mirror `bin/main.ml`).

## V ↔ Rust via Equilibrium

Equilibrium (`../equilibrium`, crate **`equilibrium-ffi`**) compiles foreign sources (including **`.v`**) to a loadable module and can feed binding generation — see `equilibrium/docs/USAGE.md`.

**Options:**

1. **build.rs / dev-only:** `equilibrium_ffi::load` or `compile_to_c` on `shared/protocol/generate_models.v` (or a thin V shim) so codegen runs inside Cargo instead of shelling `v run`.
2. **Keep V script, drop runtime `v`:** Port `generate_models.v` to **Rust** (pure `serde_json`) — smallest crates.io story; Equilibrium optional.

**Cargo sketch (local workspace only, not crates.io):**

```toml
[build-dependencies]
equilibrium-ffi = { path = "../../equilibrium" }
```

Pin paths relative to `in-cli` or a workspace root `crates/protocol-gen`.

## Swift preview host client

Applying patches touches **SwiftUI / actor state** — that stays **Swift** unless you rewrite the whole preview model.

| Approach | Use when |
|----------|----------|
| **swift-rs** | Rust **calls into** a Swift library / packaged `.dylib` that exposes C-ish entrypoints for `PreviewHost`-like operations |
| **Rust-only socket client** | **Smoke tests / headless**: read envelopes, validate JSON, log — **cannot** drive SwiftUI |
| **Status quo** | `swift run swift-preview-host-client` — minimal coupling |

**Recommendation:** Keep Swift for **apply**; if you want fewer subprocesses, add **swift-rs** bridge around a tiny Swift static library built by SPM, callable from `in dev`. A **full Rust rewrite** of preview semantics only pays off if you abandon SwiftUI-hosted previews.

## Suggested phases

1. **Phase A — Contract freeze:** Document JSON schema + OCaml artifact shape as the only cross-language API (already mostly true).
2. **Phase B — Rust protocol codegen:** Replace `generate_models.v` with Rust OR Equilibrium-driven build.rs so CI does not depend on `v` on PATH.
3. **Phase C — OCaml embed:** Link OCaml runtime + wrap `program_to_json` via **ocaml-interop**, with fallback subprocess env flag.
4. **Phase D — Preview bridge:** SPM-built Swift shim + **swift-rs** from `in dev`, or Rust headless client **only** for daemon/protocol tests.

## Related paths

- Equilibrium repo: `../equilibrium` (this machine layout).
- OCaml front entry: `compiler/ocaml-front/bin/main.ml`.
- Socket client reference: `runtime/swift-preview-host/Sources/SwiftPreviewHostClient/main.swift`.
