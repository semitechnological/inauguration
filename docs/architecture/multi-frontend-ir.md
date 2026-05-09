# Multi-frontend IR (conceptual)

Multiple source languages can feed the same hybrid pipeline by lowering to **textual SIL** that `hybrid_sil` already parses (`sil @…`, `bbN:`, `function_ref @…`, unique SSA ids across the merged module).

## `UnifiedModule` (v0 schema)

Rust type: `in_cli::core_ir::UnifiedModule`.

| Field | Meaning |
|-------|--------|
| `decls: Vec<Decl>` | Top-level declarations in source order (before lowering sorts functions for SIL). |

**`Decl` variants (v0)**

| Variant | Fields | Notes |
|---------|--------|-------|
| `Struct` | `name`, `fields: Vec<(String, Typ)>` | v0 `.in` parser emits empty `fields`; checker still validates field types when non-empty. |
| `Function` | `name`, `params`, `ret`, `body` | `body` may be empty until a front fills statements; lowering uses signatures + names only for stub SIL. |

**`Typ`**: `Int`, `String`, `Bool`, `Void`, `Named(String)` — shared with `swift_subset` today for consistency.

## `ParserId` (extensible)

| Id | Source | Entry |
|----|--------|--------|
| `In` | `.in` files | `in_lang_parse` → `UnifiedModule` → `lower_core::lower_to_textual_sil` |

Future rows: Python, Ruby, etc., each implementing `SourceParser` and gaining a `ParserId` variant when wired.

## Resolution order for `in build`

1. **`--parser in`** — force the `.in` front (even if the extension is not `.in`).
2. **Environment** — `IN_PARSER=in` (same effect as forcing the in-parser when set).
3. **Path** — if `--parser auto` (default), a path ending in **`.in`** selects `ParserId::In`.
4. **Magic line** (stub) — future first-line marker in source before extension fallback.
5. Otherwise **Swift** path: `sil_emit::emit_textual_sil` (`swiftc` and/or `IN_NATIVE_SWIFT_SIL` subset).

Documented in `in-cli/src/parser_registry.rs` as `resolve_parser_id`.

## Related

- [in-language.md](in-language.md) — `.in` v0 grammar and ideology vs crepuscularity.
- [native-swift-master-plan.md](native-swift-master-plan.md) — Rust-first Swift / subset roadmap.
