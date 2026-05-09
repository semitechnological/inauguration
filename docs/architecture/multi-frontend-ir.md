# Multi-frontend IR (Core contract)

Several **source fronts** (`.in` today, Swift gather + SIL emit by default) converge on one **cross-frontend** representation before optional lowering. This doc names the contract and how **parser resolution** picks a front.

## `UnifiedModule` / core decls

The **core IR** is a single-module view:

- **`UnifiedModule`** — ordered list of top-level **`Decl`** values.
- **`Decl`**
  - **`Struct`** — name + field list `(name, Typ)` (fields may be empty where the front does not parse them yet).
  - **`Function`** — name, parameters `(name, Typ)`, return **`Typ`**, **body** as `Vec<Stmt>` (often empty until that front grows statements).
- **`Typ`**, **`Stmt`** — shared with the Swift subset checker today (re-exported from `swift_subset` into `core_ir` so one type universe backs multiple parsers).

Lowers (e.g. to **textual SIL** stubs) take **`&UnifiedModule`** and treat missing bodies as templates until statement lowering exists.

## `ParserId` (extensible table)

| `ParserId` | Source kind | Status |
|------------|-------------|--------|
| **`In`** | `.in` v0 line-oriented (`fn` / `struct`) | **Active** |
| *future* **`Python`** | e.g. `.in.py` or magic-line dispatch | Reserved |
| *future* **`Ruby`** | e.g. `.in.rb` or magic-line dispatch | Reserved |

New fronts extend the enum, implement **`SourceParser::parse_to_core`**, and return **`UnifiedModule`** so the rest of the pipeline stays parser-agnostic.

## Resolution order (`in build`)

Today’s resolution order is deterministic and documented in code comments:

1. **`--parser in`** ⇒ **`.in` front** (`ParserId::In`).
2. **`IN_PARSER=in`** (env) ⇒ same as (1) when CLI is `auto`.
3. **`--parser auto`** (default) + path extension **`.in`** ⇒ **`.in` front**.
4. Otherwise ⇒ **Swift SIL emit** path (gather Swift sources, `swiftc` and/or **`IN_NATIVE_SWIFT_SIL`** subset); **no** `UnifiedModule` from the registry — lowering uses SIL, not core IR.

Future: optional **magic first line** or second extension could force a front before falling back to Swift.

## Related

- `.in` surface: [in-language.md](in-language.md).
- Swift subset contract: [subset-grammar.md](subset-grammar.md).
- Polyglot embedding context: [interop-roadmap.md](interop-roadmap.md).
