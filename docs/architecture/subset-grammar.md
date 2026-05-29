# Subset grammar contract (`IN_NATIVE_SWIFT_SIL=only`)

This file is the **frozen public contract** for inauguration’s in-tree Swift-shaped front when **`IN_NATIVE_SWIFT_SIL=only`** is set: **`in build`** must succeed **without `swiftc`** only for sources that match the rules below. For the roadmap, see [native-swift-master-plan.md](native-swift-master-plan.md).

Implementation today: `in-cli` modules **`native_swift_sil`** (filter + emit) and **`swift_subset`** (parse + check). Behavior outside this document is not guaranteed.

## Pipeline (what actually runs)

1. **Filter** (`filter_top_level_decl_lines`): line-oriented pass over the combined Swift inputs. It tracks `{` / `}` **brace depth** on each physical line (not inside strings—no lexer yet).
2. **Drop** at any depth: empty lines; whole lines whose trimmed form starts with `//`.
3. **Drop** at any depth: trimmed lines starting with `import ` (imports are **ignored** for subset purposes; they are not parsed as declarations).
4. **Keep** only when **brace depth is 0 before applying this line’s brace delta** and the trimmed line starts with `func ` or `struct `. Top-level function bodies are retained until their braces balance so bounded body statements can lower.
5. **Parse** the filtered text with **`swift_subset::parse`**: one top-level decl per non-empty trimmed line (see below).
6. **Check** with **`swift_subset::check`**. Any diagnostic ⇒ **`only`** mode fails (no `swiftc` fallback).

## Guaranteed syntax (after filtering)

**Line orientation**

- Only **top-level** `struct` and `func` headers contribute declarations. Everything else is ignored by the filter (except that brace depth still updates for lines that contain `{` / `}`).
- **Nested** `func` / `struct` inside `{ … }` are **not** collected; only lines at depth 0 count.

**`struct`**

- A single line: trimmed text starts with `struct `.
- The struct **name** is the token between `struct ` and `{` when `{ … }` appears. Inner text is comma-, semicolon-, or newline-separated **`name: Type`** fields (same tokens as `func` parameters). If there is no `{` / `}` pair, the name is the rest of the line after `struct ` and **fields stay empty** (legacy one-token form).
- **Multi-line** `struct { … }` field capture is supported when the top-level `struct` block is brace-balanced.

**`func`**

- A single line: trimmed text starts with `func ` (after optional leading **`public` / `private` / …** access keywords and optional **`async`**, **`throws`**, **`reasync`**, **`nonisolated`** tokens with spaces, each stripped in a bounded loop so `async throws func main() -> Void` is accepted).
- **Name** and **parameters**: `func name(_ p1: T1, p2: T2) -> Ret` — parameter list between `(` and `)`; each parameter is `label: Type` split on the first `:` (see implementation). Empty `()` allowed.
- **Return type**: if the substring after `)` contains `-> Type`, that `Type` is used; otherwise return type is **`Void`**.
- **Body** may be omitted or may appear in a brace-balanced top-level function block. The subset accepts bounded body statements: `let name = expr`, `let name: Type = expr`, `name = expr`, expression calls, `return`, `return expr`, and `if condition { ... } else { ... }`.
- **Expressions** in those bounded statements accept int/string/bool literals, identifiers, calls, and simple binary operators used by the shared Core IR lowering path.

**Types** (checker)

- Built-ins: **`Int`**, **`String`**, **`Bool`**, **`Void`**.
- **Named** types: any other identifier; must match a **struct name** declared somewhere in the same filtered program (order does not matter for the current struct set).

**Program rules** (checker)

- A top-level function named **`main`** is optional. Hybrid/library-style subset files may omit `main`; executable-style files may include it.
- No duplicate top-level names: struct names and function names share one namespace; duplicates are errors.
- No duplicate **field** names within one struct (`E_DUP_FIELD`).
- Parameter, return, and **struct field** types must be “known” (built-in or declared struct).
- Function calls inside bounded bodies must target a declared function, pass the exact parameter count (`E_CALL_ARITY`), and pass arguments whose inferred subset types match the declared parameter types (`E_CALL_ARG_TYPE`).
- `if` conditions inside bounded bodies must infer to **`Bool`** (`E_IF_COND_TYPE`).

## Explicit non-support (do not rely on these)

Treat the following as **out of contract** unless a future phase updates this document:

- **`swiftc`** / full Swift semantics, SIL compatibility with Apple’s compiler, ObjC/SwiftUI interop, modules beyond “single combined source text”.
- **`enum`**, **`class`**, **`actor`**, **`protocol`**, **`extension`**, **`typealias`**, **`var`/`let`**, computed properties, **initializers**, **deinit**, accessors.
- **Generics**, **`where`**, opaque types, **`some`/`any`**, **`async`/`await`**, **`throws`**, **`inout`**, **`mutating`**, **default arguments**, **`@`** attributes, property wrappers, **macros**.
- **Import** semantics: `import` lines are **skipped**; symbols from SDKs are **not** modeled.
- **Nested** types or functions as subset decls (filtered out).
- **Multi-line** declarations (header split across lines): not recognized; only complete headers on **one logical line** after trim.
- **String-literal-aware** brace matching: `{`/`}` inside strings still affect depth.
- Nested declarations, closures, member access, subscripts, operators outside the bounded expression set, and Swift-specific statement semantics.

## Related paths

- Sample: `apps/native-subset-sample/App.swift`
- CI: workflow job **`native-subset-sample`** (see `.github/workflows/ci.yml`)
- Local check: `./scripts/check-native-subset-sample.sh`
