# General multi-language compiler (inauguration)

`in` is evolving into a **general hybrid compiler driver**: many **source fronts** converge on shared **Core IR** ([`UnifiedModule`](multi-frontend-ir.md)), then one **SIL lowering** path feeds the existing **`hybrid_sil`** pipeline (textual SIL → passes → metrics).

The scope is compiler and backend infrastructure. Inauguration owns source-front routing, Core IR, textual SIL, graph facts, agent reports, bytecode subset execution, hot reload/compiler orchestration, and runtime-boundary reporting. Crepuscularity owns frontend UI, declarative view trees, rendering, and cross-platform visual abstraction; it should consume this compiler infrastructure rather than move UI ownership into inauguration.

This is **not** “30 production compilers in one repo overnight.” It is an **architecture** for landing languages incrementally while keeping one observable pipeline. The current source-of-truth matrix is available through **`in languages`** and **`in languages --json`**; the phased universal roadmap lives in [universal-compiler-roadmap.md](universal-compiler-roadmap.md).

## Layers

| Layer | Role today | Direction |
|-------|------------|------------|
| **Resolution** | `parser_registry`: CLI, `IN_PARSER`, magic line, extension → [`ParserId`](../../in-cli/src/parser_registry.rs) | Add `--parser` / env aliases as new fronts stabilize |
| **Parse** | Per-language modules → `UnifiedModule` | Full parsers: [`.in`](../../in-cli/src/in_lang_parse.rs) with agent-facing imports/capabilities/extern bindings, [**icore** JSON](../../in-cli/src/compiler/icore.rs). Dedicated fronts: [`rust_front`](../../in-cli/src/compiler/rust_front.rs), [`go_front`](../../in-cli/src/compiler/go_front.rs), [`v_front`](../../in-cli/src/compiler/v_front.rs), [`ocaml_front`](../../in-cli/src/compiler/ocaml_front.rs) (real declarations + bounded body-subset lowering; not full language semantics yet). Tree-sitter fronts: [`compiler::tree_front`](../../in-cli/src/compiler/tree_front/mod.rs) (Java/Groovy bounded body subset plus declaration extraction for other routed fronts). Unsupported ids still route to `.icore`. |
| **Driver** | [`compiler::driver`](../../in-cli/src/compiler/driver.rs) — Core IR → textual SIL | Shared by every front that emits `UnifiedModule` |
| **Emit (Swift)** | `sil_emit` when no Core IR front applies | Stays the `swiftc` / subset escape hatch |
| **Pipeline** | `hybrid_pipeline` / `hybrid_sil` | Merged textual SIL with explicit per-function records while retaining the legacy last-`sil @` single-function view (see [in-language.md](in-language.md)) |
| **rust-driver** | Mirror of in-cli crates | Same IR/SIL contracts as fronts mature |
| **Hot reload** | Swift: `swiftc -typecheck`; Core IR paths: `resolve_parser_id` + `parse_with_resolved` | Tighten semantics as polyglot lowering fills bodies and diagnostics |

## v0.4 orchestration/status surfaces

The general compiler roadmap should expose orchestration in strict surfaces before widening execution claims:

| Surface | Compiler role | Status rule |
|---------|---------------|-------------|
| Canonicalization | Use the parser/IR path to produce deterministic source or a rejected diagnostic set. | `in canonicalize --path <file> [--check]` is the shipped source-format surface. |
| Graph command | Report parser decision, declarations, textual SIL functions, call edges, effects, capabilities, orchestration facts, and timing. | `in graph --path <file> [--imports] [--capabilities] [--symbols] [--calls] [--json]` matches the stable Core IR graph facts. |
| Package manifest report | Report package identity, targets, dependencies, capabilities, extensions, package graph nodes, target selection, and capability policy. | `in package --path <dir\|manifest\|source> [--json]` reports package metadata and graph facts. It does not perform dependency installation or extension loading. |
| Orchestration facts | Surface `.in` extension, annotation, distributed-function, parallel-region, local plan, and local distributed job facts in agent/graph JSON. | `in agent` and `in graph --json` expose orchestration facts. `distributed-workers` has a deterministic local simulator boundary; GPU/native execution remains unavailable until runtime code and tests land. |

GPU execution, native machine-code execution, remote distributed execution, and non-owned language runtimes are status/contract-only until in-tree runtime code and tests back the claim. See [orchestration-compiler.md](orchestration-compiler.md) and [native-backend.md](native-backend.md).

## icore (JSON Core IR)

Versioned schemas (see samples under `apps/icore-sample/`):

- `icoreVersion: 1`: stable declaration interchange. Top-level `decls` contains `{"kind":"struct",...}` or `{"kind":"function",...}`. Function `body` must be **`[]`**; non-empty bodies are rejected with a v1-specific diagnostic.
- `icoreVersion: 2`: bounded body interchange for tools and plugins that can emit Core IR directly. Function `body` accepts statement objects:
  - `{ "kind": "return" }` or `{ "kind": "return", "value": <expr> }`
  - `{ "kind": "assign", "target": "name", "value": <expr> }`
  - `{ "kind": "let", "name": "name", "type": "Int", "value": <expr> }`
  - `{ "kind": "expr", "expr": <expr> }` or a direct call statement
- v2 expressions accept int/string/bool scalar literals, typed literal objects (`int`, `string`, `bool`), identifiers (`ident` / `identifier`), and calls (`{ "kind": "call", "callee": "name", "args": [...] }`). Unsupported shapes fail closed instead of inventing semantics.

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
- [orchestration-compiler.md](orchestration-compiler.md) — v0.4 orchestration/status contract
- [universal-compiler-roadmap.md](universal-compiler-roadmap.md) — full language/runtime ambition and compatibility ladder
- [parser-surface.md](parser-surface.md) — extensions, magic line, routing matrix  
- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule` schema  
- [README · in build](../../README.md#in-build-and-swiftpm-staging-macoslinux) — CLI flags
