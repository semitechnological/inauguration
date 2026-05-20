# `.in` language (v0.2 today)

The **`.in`** front is inauguration’s **brace + line-oriented** companion to the sibling **crepuscularity** project ([`../crepuscularity`](../crepuscularity)): same *ideology* — ultraminimal surface, indentation-ready grammar evolution, TypeScript-flavored expressions and interpreters in the long run — but shipped here first as something **`in build` can lower to textual SIL** without `swiftc`.

Workflow entry points (flags, sample path, CI script) stay in the repo [README](../README.md#in-build-and-swiftpm-staging-macoslinux); this page is grammar + IR shape only.

## Ideology (aligned with crepuscularity)

- **Ultraminimal**: top-level declarations first; **v0.2** supports a bounded statement/expression body subset in Core IR.
- **Indent-first on the roadmap**: crepuscularity’s **`.crepus`** files lean indent-first; `.in` v0 intentionally accepts familiar **braces + line breaks** so we can reuse the same brace-depth filtering pattern as `native_swift_sil` before tightening the grammar.
- **TS-flavored**: future expression forms can track a small JS/TS-like subset (see crepuscularity README under the repo-root symlink `../crepuscularity`).

## Current behavior (v0.2)

What `in-cli/src/in_lang_parse.rs` implements today:

- Top-level **`struct Name { … }`** — fields can appear inline or on their own lines between braces. Fields are **`Type fieldName`** segments separated by semicolons or line breaks (e.g. `struct Box { Int x; String label }`). Types must be built-ins or **struct names already declared above** in the file.
- Top-level **`fn name(params) -> Ret`** — **`fn` only** (no `func`, no `function` keyword in v0).
- Parameters: **`param: Type`** comma-separated.
- Types: **`Int`**, **`String`**, **`Bool`**, **`void` / `Void`** (`void` matching is ASCII case-insensitive), and **named structs** declared above.
- **`fn main`** is required (same spirit as the Swift subset front).
- **Function bodies**: optional brace bodies support `let`, assignment, `return`, call expressions, simple literals, identifiers, and expression statements. Lowering (`in-cli/src/lower_core.rs`) emits bounded textual SIL from non-empty Core IR bodies and keeps the sorted helper `sil @` functions plus `@main` call graph shape.
- Nesting: lines inside **`{` … `}`** are ignored for **declaration discovery** when brace-depth ≠ 0 (nested `fn` lines are not top-level), matching the Swift subset filter.

Optional spellings for forward compatibility: **`function`** as an alias may appear later; v0 **does not** accept it. **`Int`** lowers with the same **`Int64`** stub vocabulary as the Swift subset / Core IR `Typ`.

## Planned

- **Richer `fn` bodies**: control flow, richer expression operators, and sharper diagnostics.
- **Parser overrides / discovery**: today **`--parser in`**, **`IN_PARSER=in`**, or path **`*.in`** under `--parser auto` select the `.in` front (`in-cli/src/parser_registry.rs`). A **magic first-line** (or similar) before extension fallback is still a stub for mixed or extensionless paths.

Until a feature is implemented in `in_lang_parse.rs` / `lower_core.rs`, treat roadmap bullets as **targets**, not guarantees.

## `hybrid_sil` and merged textual SIL

The pipeline’s `parse_textual_sil` view keeps the legacy “last `sil @…` wins” `SilArtifact::function_id`, but merged blobs now also carry explicit per-function records in `SilArtifact::functions`. `extract_call_graph` uses those records before falling back to instruction-level callers or the legacy single-id behavior. Multi-function emitters (including `lower_to_textual_sil`) still order **`@main` last** so older single-function views stay labeled `main`. See also [multi-frontend-ir.md](multi-frontend-ir.md).

## See also

- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule`, parser resolution, SIL caveat.
- `in-cli/src/in_lang_parse.rs` — parser implementation.
- `in-cli/src/lower_core.rs` — Core IR → textual SIL.
