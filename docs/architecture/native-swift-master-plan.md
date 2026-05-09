# Native Swift / in-tree compiler master plan

This document is the phased roadmap for growing inauguration’s **Rust-first** Swift front while keeping **`swiftc`** as an escape hatch for full language semantics. It incorporates a snapshot inventory from parallel codebase exploration (in-cli, rust-driver, hotreload).

## Goals and non-goals

**Goals**

- Increase the fraction of developer workflows that **never spawn `swiftc`** for SIL or typecheck when sources match an expanding **contracted language**.
- Keep **`in build`** and future tooling **deterministic**, **fast**, and **observable** (timings, reason codes).
- Unify artifacts where practical: **subset AST → textual SIL** shapes that **`hybrid_sil`** already parses (`sil @…`, `bbN:`, `function_ref @…`).
- Eventually feed **hotreload** with the same front-end decision stack as **`in build`** (subset vs toolchain).

**Non-goals (near term)**

- Bit-for-bit SIL compatibility with Apple **`swiftc`** for arbitrary SwiftUI / ObjC interop packages.
- Replacing LLVM IRGen or the Swift runtime inside this repo.
- Checking in a full **`vendor/swift`** tree (remain optional / external).

## Current state (baseline)

| Layer | Role today |
|-------|------------|
| **`swift_subset`** | Line-oriented `struct` / `func` headers; dummy bodies; JSON artifact for **`in ocaml`**. |
| **`native_swift_sil`** | Brace-depth filter → subset parse/check → **template textual SIL** (stubs + `main` calling `function_ref`). |
| **`sil_emit`** | **`IN_NATIVE_SWIFT_SIL`**: `try` / `only`; else **`swiftc -emit-sil`** (+ SwiftPM prep / `-I` retry). |
| **`hybrid_pipeline` / `hybrid_sil`** | **`parse_textual_sil`** → strip `debug_value` → **`extract_call_graph`** from `function_ref`; Ast/Swift tasks time-only. |
| **Hotreload** | **`swiftc -typecheck`** per patch only; **no** subset / SIL reuse yet. |
| **Publishing** | **`in-cli`** mirrors `compiler/rust-driver/crates/*`; changes often land in **both** places until extraction is automated. |

**Test surface (subset/SIL slice)** is thin (~6 targeted tests across `swift_subset`, `native_swift_sil`, `sil_emit`); expand with each phase.

Also see [**multi-frontend IR**](multi-frontend-ir.md) (`UnifiedModule`, `.in` parser id, resolution order).

---

## Phase 0 — Contracts and tooling (1–2 weeks)

**Deliverables**

- Frozen **subset grammar** (`docs/architecture/subset-grammar.md`): what is guaranteed under `IN_NATIVE_SWIFT_SIL=only`.
- Manual hybrid mirror checklist: **[contributing-hybrid-mirror.md](../contributing-hybrid-mirror.md)** (diff examples; drift breaks publish parity).
- CI job matrix entry: **`IN_NATIVE_SWIFT_SIL=only`** build of **`apps/native-subset-sample`** (no **`swiftc`** required on runner).

**Exit criteria**

- Contributors can answer “is this in subset?” without reading Rust: see **[subset-grammar.md](subset-grammar.md)** (frozen contract for `IN_NATIVE_SWIFT_SIL=only`).
- CI job **`native-subset-sample`** (`.github/workflows/ci.yml`) exercises `apps/native-subset-sample` with **`IN_NATIVE_SWIFT_SIL=only`** on Ubuntu and macOS (no **`swiftc`** invocation on the success path).

---

## Phase 1 — Real AST for the subset (3–6 weeks)

**Problem**: Headers-only parsing; `struct` fields empty; bodies are placeholders.

**Workstreams** (parallel-friendly)

1. **Lexer + recursive descent** (or `logos`/`pest` gated behind feature): top-level decls, **`struct` fields**, **`enum` cases** (optional minimal), **`func` signatures**.
2. **Body statements**: `let`, `return`, expression subset (`call`, literals, binary `+`/`-` on `Int`).
3. **Name resolution**: scopes, shadowing rules, `unknown identifier` diagnostics aligned with checker style today.

**SIL lowering (incremental)**

- Per-function **`bb0`** with real **`apply`**-shaped pseudo-ops or **`function_ref` + `apply`** lines that **`extract_call_graph`** still sees.
- Typed literals per return type stub (`Int` vs `Void`).

**Tests**

- Parser corpus under **`in-cli/tests/fixtures/subset/*.swift`** with expected diagnostics / SIL snapshots (string compare).

**Exit criteria**

- Sample multi-function programs with **calls** and **struct fields** pass **`only`** mode and produce **non-trivial call graphs** in **`hybrid_sil`**.

---

## Phase 2 — Types and safe diagnostics (4–8 weeks)

**Deliverables**

- Built-in types + **user structs**; function arity and argument type checks; optional **`mutating`** / **`inout`** later.
- **Borrow checker lite** or copy-only semantics explicitly documented (no implicit ARC modeling until chosen).
- Structured **`Diagnostic`** spans (file, line, column) once lexer tracks offsets.

**Integration**

- Wire **`summarize_frontend_artifact`** from **`hybrid_pipeline`** when emitting JSON from subset (today unused in wave — natural hook per scheduler design).

**Exit criteria**

- Reject ill-typed subset programs with stable error codes; **`try`** mode reliably falls back when diagnostics indicate “not subset” vs “subset error”.

---

## Phase 3 — Pipeline as product (3–5 weeks)

**Deliverables**

- Replace discarded **`_optimized` / `_report`** in **`run_wave_with_timings`** with pluggable **SIL passes** (trait in **`hybrid_sil`** / **`sil`** crate): retain graph, expose summary JSON for **`--verbose`**.
- Optional **second SIL round-trip**: subset emitter outputs format Version header for forward compatibility.
- Metrics: **`swift_frontend_us`** renamed or split into **`sil_emit_us`** + **`sil_frontend_kind`** (`native_subset` | `swiftc`).

**Exit criteria**

- **`in build --verbose`** shows actionable SIL analysis summary, not only timings.

---

## Phase 4 — Hotreload alignment (4–6 weeks)

**Deliverables**

- **`compile_check_cached`** path: try **`native_swift_sil` / subset check** first when env matches **`in build`**; fallback **`swiftc -typecheck`**.
- Cache keys include **frontend kind** + normalized source hash.
- Optional NDJSON metric field: **`compile_frontend`** (`subset` | `swiftc`).

**Exit criteria**

- Reload loop latency improves on subset-only sample apps; behavior documented in README.

---

## Phase 5 — Scale-out / maintenance (ongoing)

- **Macro expansion**: none initially; explicit **`@`** attributes stripped or rejected with clear errors.
- **Module system**: `import` ignored vs honored per grammar appendix.
- **Codegen**: only after Phase 2 stable — consider MLIR / custom bytecode **only** if textual SIL becomes the bottleneck.

---

## Risk register

| Risk | Mitigation |
|------|------------|
| **`hybrid_*` drift** | CI diff or xtask mirror; long-term: single crate dependency from **`in-cli`** to workspace packages when publishing allows. |
| **SIL parser fragility** | Golden-file tests; versioned SIL header from subset emitter. |
| **User expects full Swift under `only`** | Docs + diagnostics pointing to **`try`** / **`swiftc`**. |
| **Hotreload false negatives** | Conservative fallback to **`swiftc`** on any unknown construct. |

---

## Subagent / parallel work breakdown (for future execution)

When spawning coding agents, partition by **Phase 1 workstreams** above:

| Agent | Scope |
|-------|--------|
| **A** | `swift_subset`: grammar, parser, tests |
| **B** | `native_swift_sil`: lowering strategy, SIL snapshots |
| **C** | `compiler/rust-driver/crates/sil` + `pipeline`: traits, non-discarded analysis |
| **D** | `hotreload/daemon_impl.rs`: dual compile gate, metrics |
| **E** | Docs + CI (`IN_NATIVE_SWIFT_SIL=only` job) |

Hand-off contract between agents: **`Program` AST type** in **`swift_subset`**, **`emit_program(&Program) -> String`** signature in **`native_swift_sil`**, **`SilArtifact`** fields unchanged unless version bump coordinated.

---

## References (paths)

- `in-cli/src/swift_subset.rs`, `native_swift_sil.rs`, `sil_emit.rs`, `main.rs`
- `in-cli/src/hotreload/daemon_impl.rs`
- `compiler/rust-driver/crates/pipeline/src/lib.rs`, `sil/src/lib.rs`, `scheduler/src/lib.rs`, `core/src/lib.rs`
- `apps/native-subset-sample/App.swift`
- `docs/architecture/subset-grammar.md`
- `docs/architecture/interop-roadmap.md`
