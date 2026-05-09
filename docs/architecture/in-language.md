# `.in` language (v0)

The **`.in`** front is inauguration’s **brace + line-oriented** companion to the sibling **crepuscularity** project ([`../crepuscularity`](../crepuscularity)): same *ideology* — ultraminimal surface, indentation-ready grammar evolution, TypeScript-flavored expressions and interpreters in the long run — but shipped here first as something **`in build` can lower to textual SIL** without `swiftc`.

## Ideology (aligned with crepuscularity)

- **Ultraminimal**: top-level declarations only in v0; bodies are placeholders until the interpreter grows.
- **Indent-first on the roadmap**: crepuscularity’s **`.crepus`** files lean indent-first; `.in` v0 intentionally accepts familiar **braces + line breaks** so we can reuse the same brace-depth filtering pattern as `native_swift_sil` before tightening the grammar.
- **TS-flavored**: future expression forms can track a small JS/TS-like subset (see crepuscularity README under the repo-root symlink `../crepuscularity`).

## v0 grammar (what `in build --parser in` accepts)

- Top-level **`struct Name`** (optional `{` on the same line; fields empty in v0).
- Top-level **`fn name(params) -> Ret`** — **`fn` only** (no `func`, no `function` keyword in v0).
- Parameters: **`param: Type`** comma-separated.
- Types: **`Int`**, **`String`**, **`Bool`**, **`void` / `Void`** (return only for void; matching is ASCII case-insensitive for `void`), and **named structs** declared above use.
- **`fn main`** is required (same spirit as the Swift subset front).
- Nesting: lines inside **`{` … `}`** are ignored for declaration discovery (brace-depth ≠ 0), matching the Swift subset filter.

Optional spellings documented for forward compatibility: some tooling may accept **`function`** as an alias later; v0 **does not**. Types may be documented as **`Int`** vs Swift **`Int64`** stubs in SIL — the Core IR reuses the same `Typ` vocabulary as `swift_subset` for now.

## See also

- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule` and parser resolution.
- `in-cli/src/in_lang_parse.rs` — parser implementation.
- `in-cli/src/lower_core.rs` — Core IR → textual SIL.
