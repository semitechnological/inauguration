# General multi-language compiler (inauguration)

`in` is evolving into a **general hybrid compiler driver**: many **source fronts** converge on shared **Core IR** ([`UnifiedModule`](multi-frontend-ir.md)), then one **SIL lowering** path feeds the existing **`hybrid_sil`** pipeline (textual SIL → passes → metrics).

This is **not** “30 production compilers in one repo overnight.” It is an **architecture** for landing languages incrementally while keeping one observable pipeline.

## Layers

| Layer | Role today | Direction |
|-------|------------|------------|
| **Resolution** | `parser_registry`: CLI, `IN_PARSER`, magic line, extension → [`ParserId`](../../in-cli/src/parser_registry.rs) | Add `--parser` / env aliases as new fronts stabilize |
| **Parse** | Per-language modules → `UnifiedModule` | Full parsers: [`.in`](../../in-cli/src/in_lang_parse.rs), [**icore** JSON](../../in-cli/src/compiler/icore.rs). Dedicated fronts: [`rust_front`](../../in-cli/src/compiler/rust_front.rs), [`go_front`](../../in-cli/src/compiler/go_front.rs), [`v_front`](../../in-cli/src/compiler/v_front.rs) (real declarations + bounded body-subset lowering; not full language semantics yet). Tree-sitter fronts: [`compiler::tree_front`](../../in-cli/src/compiler/tree_front/mod.rs) (Java/Groovy bounded body subset plus declaration extraction for other routed fronts). Unsupported ids still route to `.icore`. |
| **Driver** | [`compiler::driver`](../../in-cli/src/compiler/driver.rs) — Core IR → textual SIL | Shared by every front that emits `UnifiedModule` |
| **Emit (Swift)** | `sil_emit` when no Core IR front applies | Stays the `swiftc` / subset escape hatch |
| **Pipeline** | `hybrid_pipeline` / `hybrid_sil` | Merged textual SIL with explicit per-function records while retaining the legacy last-`sil @` single-function view (see [in-language.md](in-language.md)) |
| **rust-driver** | Mirror of in-cli crates | Same IR/SIL contracts as fronts mature |
| **Hot reload** | Swift: `swiftc -typecheck`; Core IR paths: `resolve_parser_id` + `parse_with_resolved` | Tighten semantics as polyglot lowering fills bodies and diagnostics |

## icore (JSON Core IR)

Version **1** schema (see sample under `apps/icore-sample/`):

- Top-level `icoreVersion: 1` and `decls` array.
- Each element is `{"kind":"struct",...}` or `{"kind":"function",...}` with `return` (string), `params`, and **`body: []`** only (non-empty bodies rejected until v2 adds `Stmt` JSON).

Any tool may emit **icore** so inauguration runs **without** a native lexer for that language yet.

## Per-language roadmap (signatures → semantics)

For each [`ParserId`](parser-surface.md) where Tree-sitter extraction is signature-only today (most polyglot ids):

1. **Lex + parse** (hand-rolled, `logos`, `tree-sitter`, or bridge to an external AST).
2. **Lower to `UnifiedModule`** (or extend IR if the language cannot map — prefer extending `Decl` / `Stmt` rarely; add side tables instead).
3. **Reuse** `compiler::driver::lower_unified_module` until SIL needs richer ops; then extend `lower_core` with shared lowering helpers.
4. **Tests**: parser corpus + SIL substring / graph assertions.
5. **Extend** [`compiler::tree_front`](../../in-cli/src/compiler/tree_front/mod.rs) extraction (or replace it with a hand-rolled front) until statements and types round-trip into `Stmt` / richer `Typ`.

Swift-shaped work in parallel: [native-swift-master-plan.md](native-swift-master-plan.md).

## “Do the other stuff”

| Track | Work |
|-------|------|
| **hybrid_sil** | Multi-function `function_id` / graph fidelity if emitters stop relying on “`main` last” |
| **Hot reload** | Further tighten patch / graph semantics beyond compile-check gating |
| **rust-driver** | Deeper SIL / batch path; keep hybrid mirror in sync |
| **Types** | Cross-front typecheck on `UnifiedModule` or a future TIR |

## See also

- [future-work-roadmap.md](future-work-roadmap.md) — phased backlog (parsers, driver, SIL, hot reload)
- [parser-surface.md](parser-surface.md) — extensions, magic line, routing matrix  
- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule` schema  
- [README · in build](../../README.md#in-build-and-swiftpm-staging-macoslinux) — CLI flags
