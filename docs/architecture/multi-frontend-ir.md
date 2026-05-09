# Multi-frontend IR (conceptual)

Multiple source languages can feed the same hybrid pipeline by lowering to **textual SIL** that `hybrid_sil` already parses (`sil @…`, `bbN:`, `function_ref @…`, unique SSA ids across the merged module).

## Textual SIL merge caveat (`hybrid_sil`)

`parse_textual_sil` is a **single-artifact** scan: the top-level **`function_id`** is still whichever **`sil @name`** line appeared **last** in the input, and basic blocks / instructions stay one flattened list. **`extract_call_graph`**, however, can attribute each instruction to the **active `sil @…` at parse time** via parallel **`instruction_callers`** (when populated by `parse_textual_sil`); legacy artifacts with empty callers keep the old behavior (**every edge uses `function_id` only**). Emitters still typically place **`sil @main` last** so single-function views line up with `main`. For full detail see [in-language.md](in-language.md#hybrid_sil-and-merged-textual-sil).

## `UnifiedModule` (v0 schema, v0.2 extensions TBD)

Rust type: `in_cli::core_ir::UnifiedModule`.

| Field | Meaning |
|-------|--------|
| `decls: Vec<Decl>` | Top-level declarations in source order (before lowering sorts functions for SIL). |

**`Decl` variants (v0)**

| Variant | Fields | Notes |
|---------|--------|-------|
| `Struct` | `name`, `fields: Vec<(String, Typ)>` | **`.in` v0**: fields populated only from a **single-line** `struct Name { … }` body (see [in-language.md](in-language.md)). **Target v0.2**: multiline field blocks between braces. |
| `Function` | `name`, `params`, `ret`, `body` | **`.in`**: statement list when the parser fills it; lowering follows `lower_core`. |

**`Typ`**: `Int`, `String`, `Bool`, `Void`, `Named(String)` — shared with `swift_subset` today for consistency.

## `ParserId` (extensible)

| Id | Source | Entry |
|----|--------|--------|
| `In` | `.in` files (and `#!in parser=in`) | `in_lang_parse` → `UnifiedModule` → `compiler::driver` / `lower_core` |
| `Icore` | `.icore` files (and `#!in parser=icore`) | `compiler::icore` → `UnifiedModule` → same lowering |
| `c`, `cpp`, `java`, `python`, … | Known extensions or `#!in parser=<slug>` | **Tree-sitter polyglot** — [`compiler::tree_front`](../../in-cli/src/compiler/tree_front/mod.rs) grammar-backed AST → signature `UnifiedModule` (empty bodies v0); icore-only ids documented in [parser-surface.md](parser-surface.md). |

## Resolution order for `in build`

See **`in-cli/src/parser_registry.rs`** (`resolve_parser_id`) and [parser-surface.md](parser-surface.md). In short:

1. **`--parser in`** — force the `.in` front.  
2. **Magic first line** — `#!in parser=in` | `auto` | `<slug>`.  
3. **`IN_PARSER=in`**.  
4. **Extension map** — `.in`, `.icore`, `.java`, `.cpp`, … → Core IR path (full parsers for `.in`/`.icore`; Tree-sitter for other wired extensions).  
5. Otherwise **Swift** path: `sil_emit::emit_textual_sil` (`swiftc` and/or `IN_NATIVE_SWIFT_SIL` subset).

## Related

- [general-compiler.md](general-compiler.md) — multi-language driver, **icore**, roadmap.  
- [parser-surface.md](parser-surface.md) — extension + magic-line routing, Tree-sitter vs full fronts.  
- [in-language.md](in-language.md) — `.in` v0 vs v0.2 targets, grammar, `hybrid_sil` note.
- [native-swift-master-plan.md](native-swift-master-plan.md) — Rust-first Swift / subset roadmap.
- [README · `in build` / `.in`](../README.md#in-build-and-swiftpm-staging-macoslinux) — CLI flags and sample commands (no duplicate install steps here).
