# Multi-frontend IR (conceptual)

Multiple source languages can feed the same hybrid pipeline by lowering to **textual SIL** that `hybrid_sil` already parses (`sil @…`, `bbN:`, `function_ref @…`, unique SSA ids across the merged module).

## Textual SIL merge behavior (`hybrid_sil`)

`parse_textual_sil` keeps the legacy top-level **`function_id`** as whichever **`sil @name`** line appeared **last** in the input, and still exposes flattened block / instruction vectors for existing callers. It also records explicit per-function entries in **`SilArtifact::functions`**, each with its own blocks and instructions. **`extract_call_graph`** prefers those per-function records, then falls back to **`instruction_callers`**, then to the legacy **`function_id`** behavior for old artifacts. Emitters still typically place **`sil @main` last** so single-function views line up with `main`. For full detail see [in-language.md](in-language.md#hybrid_sil-and-merged-textual-sil).

## `UnifiedModule` (current schema)

Rust type: `in_cli::core_ir::UnifiedModule`.

| Field | Meaning |
|-------|--------|
| `decls: Vec<Decl>` | Top-level declarations in source order (before lowering sorts functions for SIL). |

`.in` also has a parser-side surface fact helper for `import`, `capability`, and `extern` declarations. Local relative `.in` imports merge imported declarations into the current `UnifiedModule`; imports and capabilities are exposed through `in agent` JSON, and extern `requires` contracts produce agent diagnostics when required capabilities are missing. Extern bindings lower into empty `Function` declarations today so call graph extraction can observe explicit `.in` calls without expanding the Core IR schema.

**`Decl` variants**

| Variant | Fields | Notes |
|---------|--------|-------|
| `Struct` | `name`, `fields: Vec<(String, Typ)>` | **`.in` v0.2** supports multiline field blocks; Tree-sitter and dedicated fronts fill fields where their current extractor supports them. |
| `Function` | `name`, `params`, `ret`, `body` | **`.in`**, `icoreVersion: 2`, dedicated Rust/Go/V fronts, and selected Tree-sitter fronts fill bounded statement lists; lowering follows `lower_core`. |

**`Typ`**: `Int`, `String`, `Bool`, `Void`, `Named(String)` — shared with `swift_subset` today for consistency.

## `ParserId` (extensible)

| Id | Source | Entry |
|----|--------|--------|
| `In` | `.in` files (and `#!in parser=in`) | `in_lang_parse` → `UnifiedModule` → `compiler::driver` / `lower_core`; parser-side surface facts feed agent `effects` / `capabilities` |
| `Icore` | `.icore` files (and `#!in parser=icore`) | `compiler::icore` v1 declarations or v2 bounded body JSON → `UnifiedModule` → same lowering |
| `c`, `cpp`, `java`, `python`, … | Known extensions or `#!in parser=<slug>` | **Tree-sitter polyglot** — [`compiler::tree_front`](../../in-cli/src/compiler/tree_front/mod.rs) grammar-backed AST → `UnifiedModule`; Java/Groovy and C-family have bounded body lowering where documented, other routed fronts remain declaration-level; icore-only ids documented in [parser-surface.md](parser-surface.md). |

## Resolution order for `in build`

See **`in-cli/src/parser_registry.rs`** (`resolve_parser_id`) and [parser-surface.md](parser-surface.md). In short:

1. **`--parser in` / `--parser icore`** — force the `.in` or `.icore` front.
2. **Magic first line** — `#!in parser=in` | `auto` | `<slug>`.  
3. **`IN_PARSER=in` / `IN_PARSER=icore`**.
4. **Extension map** — `.in`, `.icore`, `.java`, `.cpp`, … → Core IR path (full parsers for `.in`/`.icore`; Tree-sitter for other wired extensions).  
5. Otherwise **Swift** path: `sil_emit::emit_textual_sil` (`swiftc` and/or `IN_NATIVE_SWIFT_SIL` subset).

## Related

- [general-compiler.md](general-compiler.md) — multi-language driver, **icore**, roadmap.  
- [parser-surface.md](parser-surface.md) — extension + magic-line routing, Tree-sitter vs full fronts.  
- [in-language.md](in-language.md) — `.in` v0.2 grammar and `hybrid_sil` note.
- [native-swift-master-plan.md](native-swift-master-plan.md) — Rust-first Swift / subset roadmap.
- [README · `in build` / `.in`](../../README.md#in-build-and-swiftpm-staging-macoslinux) — CLI flags and sample commands (no duplicate install steps here).
