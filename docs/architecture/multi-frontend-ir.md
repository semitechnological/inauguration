# Multi-frontend IR (conceptual)

Multiple source languages can feed the same hybrid pipeline by lowering to **textual SIL** that `hybrid_sil` already parses (`sil @…`, `bbN:`, `function_ref @…`, unique SSA ids across the merged module).

## Textual SIL merge caveat (`hybrid_sil`)

`parse_textual_sil` is a **single-artifact** scan: the **`function_id`** it keeps is whichever **`sil @name`** line appeared **last** in the input. All basic blocks and instructions from the entire string are flattened into that one view, and **`extract_call_graph`** reports edges **from that `function_id` only**. Emitters that concatenate several functions (native subset, `lower_core`) rely on this and typically place **`sil @main` last** so graph extraction still names the merged slice `main`. For full detail see [in-language.md](in-language.md#hybrid_sil-and-merged-textual-sil).

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
| `In` | `.in` files (and `#!in parser=in`) | `in_lang_parse` → `UnifiedModule` → `lower_core::lower_to_textual_sil` |
| `c`, `cpp`, `java`, `python`, … | Known extensions or `#!in parser=<slug>` | **Stub** — [`ParserRegistryError::NotImplemented`](../../in-cli/src/parser_registry.rs); see [parser-surface.md](parser-surface.md). |

## Resolution order for `in build`

See **`in-cli/src/parser_registry.rs`** (`resolve_parser_id`) and [parser-surface.md](parser-surface.md). In short:

1. **`--parser in`** — force the `.in` front.  
2. **Magic first line** — `#!in parser=in` | `auto` | `<slug>`.  
3. **`IN_PARSER=in`**.  
4. **Extension map** — `.in`, `.java`, `.cpp`, … → Core IR path (stubs except `.in`).  
5. Otherwise **Swift** path: `sil_emit::emit_textual_sil` (`swiftc` and/or `IN_NATIVE_SWIFT_SIL` subset).

## Related

- [parser-surface.md](parser-surface.md) — extension + magic-line routing, stub fronts.  
- [in-language.md](in-language.md) — `.in` v0 vs v0.2 targets, grammar, `hybrid_sil` note.
- [native-swift-master-plan.md](native-swift-master-plan.md) — Rust-first Swift / subset roadmap.
- [README · `in build` / `.in`](../README.md#in-build-and-swiftpm-staging-macoslinux) — CLI flags and sample commands (no duplicate install steps here).
