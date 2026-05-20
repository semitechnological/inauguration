# `.in` language (v0 today, v0.2 on the roadmap)

The **`.in`** front is inauguration’s **brace + line-oriented** companion to the sibling **crepuscularity** project ([`../crepuscularity`](../crepuscularity)): same *ideology* — ultraminimal surface, indentation-ready grammar evolution, TypeScript-flavored expressions and interpreters in the long run — but shipped here first as something **`in build` can lower to textual SIL** without `swiftc`.

Workflow entry points (flags, sample path, CI script) stay in the repo [README](../README.md#in-build-and-swiftpm-staging-macoslinux); this page is grammar + IR shape only.

## Ideology (aligned with crepuscularity)

- **Ultraminimal**: top-level declarations first; **v0** keeps **`fn` bodies empty** in Core IR until statements land.
- **Indent-first on the roadmap**: crepuscularity’s **`.crepus`** files lean indent-first; `.in` v0 intentionally accepts familiar **braces + line breaks** so we can reuse the same brace-depth filtering pattern as `native_swift_sil` before tightening the grammar.
- **TS-flavored**: future expression forms can track a small JS/TS-like subset (see crepuscularity README under the repo-root symlink `../crepuscularity`).

## Current behavior (v0)

What `in-cli/src/in_lang_parse.rs` implements today:

- Top-level **`struct Name { … }`** — **`{` and `}` must appear on the same line as the field list**. Inside the braces, fields are **`Type fieldName`** segments separated by **`;`** (e.g. `struct Box { Int x; String label }`). Types must be built-ins or **struct names already declared above** in the file.
- Top-level **`fn name(params) -> Ret`** — **`fn` only** (no `func`, no `function` keyword in v0).
- Parameters: **`param: Type`** comma-separated.
- Types: **`Int`**, **`String`**, **`Bool`**, **`void` / `Void`** (`void` matching is ASCII case-insensitive), and **named structs** declared above.
- **`fn main`** is required (same spirit as the Swift subset front).
- **Function bodies**: not parsed; Core IR stores an **empty** statement list. Lowering (`in-cli/src/lower_core.rs`) emits the same **stub SIL** pattern as the multi-function subset path (sorted helper `sil @` functions, then `@main` with `function_ref` edges for call-graph tests).
- Nesting: lines inside **`{` … `}`** are ignored for **declaration discovery** when brace-depth ≠ 0 (nested `fn` lines are not top-level), matching the Swift subset filter.

Optional spellings for forward compatibility: **`function`** as an alias may appear later; v0 **does not** accept it. **`Int`** lowers with the same **`Int64`** stub vocabulary as the Swift subset / Core IR `Typ`.

## Planned (v0.2 target — sync with code as it lands)

- **Multiline struct fields**: allow fields on their own lines between `{` and `}` (today the parser only reads the single-line `{ … }` span).
- **Richer `fn` bodies**: statements, expressions, and diagnostics; lowering would graduate from stubs toward real SIL shapes over time.
- **Parser overrides / discovery**: today **`--parser in`**, **`IN_PARSER=in`**, or path **`*.in`** under `--parser auto` select the `.in` front (`in-cli/src/parser_registry.rs`). A **magic first-line** (or similar) before extension fallback is still a stub for mixed or extensionless paths.

Until a feature is implemented in `in_lang_parse.rs` / `lower_core.rs`, treat roadmap bullets as **targets**, not guarantees.

## `hybrid_sil` and merged textual SIL

The pipeline’s `parse_textual_sil` view keeps the legacy “last `sil @…` wins” `SilArtifact::function_id`, but merged blobs now also carry explicit per-function records in `SilArtifact::functions`. `extract_call_graph` uses those records before falling back to instruction-level callers or the legacy single-id behavior. Multi-function emitters (including `lower_to_textual_sil`) still order **`@main` last** so older single-function views stay labeled `main`. See also [multi-frontend-ir.md](multi-frontend-ir.md).

## See also

- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule`, parser resolution, SIL caveat.
- `in-cli/src/in_lang_parse.rs` — parser implementation.
- `in-cli/src/lower_core.rs` — Core IR → textual SIL.
