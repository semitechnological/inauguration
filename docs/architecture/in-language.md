# `.in` v0 (line-oriented front)

Experimental **ultraminimal** surface for inauguration: **TypeScript-flavored** keywords and types (`fn`, `struct`, `Int`, explicit `->` returns), designed so a single file can be **filtered** to top-level headers without a full lexer yet.

## Ideology

- **Small surface area:** only what `in build` needs to reach **core IR** and lowering experiments.
- **TS-adjacent ergonomics:** `fn name(a: T) -> U` reads like a type annotation, not Swift’s `func`/`Void` spellings (though `Void` / case-insensitive `void` are accepted as return types).
- **Indent-first future:** long term, **crepuscularity**’s [`.crepus` templates](https://github.com/semitechnological/crepuscularity) favor **indentation-structured** authoring over brace bookkeeping in the author’s head. `.in` stays **line + brace-depth** for v0, but the design bias is the same: **keep authoring shallow and tool-obvious**. If you have a local clone next to this repo, see `../crepuscularity/README.md`.

## Grammar (v0)

**Brace filter** (same *idea* as the Swift subset, different keywords):

1. Walk the file line by line; track `{` / `}` brace depth on each **physical line** (not string-aware).
2. Skip empty lines and lines whose trim starts with `//`.
3. When depth is **0** before applying this line’s brace delta, **keep** lines that trim to `fn …` or `struct …`; all other lines are dropped for declaration extraction. Nested `fn` / `struct` inside `{ … }` are ignored.

**After filtering**, each remaining non-empty line is one declaration:

- **`struct Name`** or **`struct Name {`** — name only today; fields are not parsed from the line.
- **`fn name(param: Type, …) -> Ret`** — parameters are `label: Type` segments separated by `,`; return is optional; if missing, return type is **Void** / `void`.

**Types** (checker)

- Built-ins: `Int`, `String`, `Bool`, `Void` (and `void` case-insensitive).
- Any other identifier is a **named** type and must match a `struct` declared in the same module (order-independent).

**Program rules**

- Must contain **`fn main`** at top level (after filtering).
- Struct and function names share one namespace; **no duplicates**.

## Related

- Core IR consumed by this front: [multi-frontend-ir.md](multi-frontend-ir.md).
- Frozen **Swift-shaped** subset (`func` / `struct`): [subset-grammar.md](subset-grammar.md).
- Implementation reference: `in-cli` modules `in_lang_parse`, `core_ir`, `lower_core`.
