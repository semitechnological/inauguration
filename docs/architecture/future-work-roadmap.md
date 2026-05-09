# Future work roadmap

## Executive summary

`inauguration` is becoming a **general hybrid compiler**: many source fronts can lower into shared Core IR, then reuse one SIL-oriented driver and `hybrid_sil` pipeline. The current model is layered: **full parsers** (`.in`, `.icore`) own rich grammars and semantics; dedicated **Rust/Go/V** fronts already lower real declarations plus bounded body subsets; and **Tree-sitter polyglot** (`compiler::tree_front`) still handles many other extensions at signature-level `UnifiedModule` (empty bodies until lowering grows). Parser ids without a compatible grammar still route contributors to **`.icore`**.

The near-term goal is not to finish every language at once. It is to keep parser routing, Core IR, SIL lowering, hot reload, and rust-driver parity moving together so each Tree-sitter front can deepen (statements, types, diagnostics) without forking the pipeline.

## Done recently

- Added dedicated fronts for **Rust**, **Go**, and **V** with real top-level lowering and body subsets.
- **`compiler::tree_front`** still covers many other `ParserId`s (signature extraction → `UnifiedModule`).
- Routed hot reload **compile_check** through parser resolution for Core IR fronts, so polyglot / `.in` / `.icore` inputs share one decision path.
- Centralized front selection in **`parser_registry`** with concrete `ParserId` handling; tracked parser ids no longer rely on `NotImplemented`-style placeholders.

## Phased roadmap

| Track | v0 near-term | v1 direction |
|-------|--------------|--------------|
| **Per-language semantics** | Deepen dedicated Rust/Go/V fronts from body subsets into richer CFG-aware lowering; in parallel, lift selected Tree-sitter fronts from signature-only to body lowering. | Dedicated semantics / typecheck layers per language family where overlap allows shared infrastructure. |
| **icore evolution** | Keep `icoreVersion: 1` stable for declarations and empty bodies; document rejected shapes clearly. | Add statement / expression JSON, versioned compatibility rules, and fixture-driven lowering tests for tool-generated IR. |
| **rust-driver parity with `in-cli`** | Mirror parser ids, Core IR contracts, and SIL lowering behavior as fronts stabilize. | Reduce drift by extracting shared crates or generated contract tests across `in-cli` and `compiler/rust-driver`. |
| **`hybrid_sil` correctness** | Preserve the current merged textual SIL contract while tests pin call-graph behavior. | Model multiple functions explicitly: stable `function_id`s, per-function blocks, and graph extraction that does not depend on the last `sil @...` line. |
| **Hot reload semantics** | Compile-check routing understands Core IR fronts and Tree-sitter polyglot parses. | Move beyond "can this compile?" into patch semantics: dependency-aware invalidation, graph-aware reload decisions, and frontend-specific diagnostics. |
| **Swift subset / native SIL** | Keep the subset as a fast, bounded path with `swiftc` fallback for full language behavior. | Grow richer Swift AST, type checking, and native SIL emission so common Swift workflows avoid `swiftc` without pretending to cover all Swift semantics. |

## v0 focus

- Pick one or two Tree-sitter fronts and drive **end-to-end lowering tests**: source → `UnifiedModule` (with growing bodies) → textual SIL → `hybrid_sil` graph.
- Keep **icore** conservative: stable v1 ingestion, better diagnostics, and examples that external tools can generate confidently.
- Add parity tests that compare `in-cli` and rust-driver outputs for the same Core IR fixtures.
- Pin the current `hybrid_sil` caveat in tests before changing representation, so graph correctness work has a safe baseline.
- Surface hot reload reason codes for parser kind, compile-check result, and fallback path.

## v1 targets

- Per-language parser maturity: promoted languages stop being signature-only when bodies and diagnostics match project standards.
- Core IR grows only where multiple fronts need the shape; otherwise language-specific detail stays in side tables or frontend artifacts.
- rust-driver becomes a first-class execution path for the same source-front contracts, not a lagging mirror.
- `hybrid_sil` treats multi-function modules as structured graphs rather than flattened text slices.
- Hot reload uses compiler knowledge to decide patchability, not only typecheck success.
- Swift subset emits more faithful SIL for real bodies, calls, user structs, and diagnostics while preserving deterministic fallback to `swiftc`.

## See also

- [../roadmap-execution-plan.md](../roadmap-execution-plan.md) — cross-stream execution order (SIL, CI, rust-driver mirror, polyglot).
- [general-compiler.md](general-compiler.md) — umbrella model for the general hybrid compiler.
- [parser-surface.md](parser-surface.md) — parser ids, extension routing, magic lines, Tree-sitter vs icore-only ids.
- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule`, `ParserId`, and the current `hybrid_sil` caveat.
- [native-swift-master-plan.md](native-swift-master-plan.md) — Swift subset and native SIL roadmap.
