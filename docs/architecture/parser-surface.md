# Parser surface (`in build`)

`in-cli/src/parser_registry.rs` resolves which **front** runs before the hybrid SIL pipeline. **Full** Core IR fronts: **`.in`**, **`.icore` (JSON)**. Dedicated language fronts now include **Rust** (`compiler::rust_front`), **Go** (`compiler::go_front`), and **V** (`compiler::v_front`) with real function/struct lowering plus bounded body subsets (not full language semantics yet). Other tracked extensions use **`compiler::tree_front`** (**Tree-sitter** grammars → `UnifiedModule`). Java is wired through Tree-sitter method extraction with bounded returns, assignments, and call expressions, plus source → textual SIL → `hybrid_sil` graph coverage. C / C++ / Objective-C++ additionally fill a bounded trivial return-body subset where wired. Parser ids without a wired grammar return an error that directs callers to `.icore` — see [general-compiler.md](general-compiler.md).

Resolution order is documented in the `parser_registry` module rustdoc. Summary:

1. `--parser in` / `--parser icore` → force that Core IR front  
2. Magic first line `#!in parser=…` on a regular file  
3. `IN_PARSER=in` or `IN_PARSER=icore`  
4. Extension map below (case-insensitive)  
5. Otherwise → Swift SIL emit (`.swift`, packages, `swiftc` / subset env)

## Magic line

- `#!in parser=in` — force `.in` grammar (even if the extension is not `.in`).  
- `#!in parser=auto` — defer to steps 3–5.  
- `#!in parser=<slug>` — force a tracked front when `<slug>` matches a [`ParserId`](../../in-cli/src/parser_registry.rs) (`java`, `cpp`, `objc`, …). Unrecognized values are ignored (fall through).

## Extension → front (dedicated front when available, otherwise Tree-sitter polyglot)

| Family | Extensions | `ParserId` |
|--------|------------|------------|
| inauguration | `in` | `in` (**implemented**) |
| Core IR JSON | `icore` | `icore` (**implemented** — [general-compiler.md](general-compiler.md)) |
| C / headers | `c`, `h` | `c` |
| C++ | `cc`, `cpp`, `cxx`, `hpp`, `hxx`, `hh`, `h++`, `ipp` | `cpp` |
| Objective-C | `m` | `objc` |
| Objective-C++ | `mm` | `objc++` |
| Java | `java` | `java` |
| Kotlin | `kt`, `kts` | `kotlin` |
| Scala | `scala`, `sc` | `scala` |
| C# | `cs` | `csharp` |
| F# | `fs`, `fsx`, `fsi` | `fsharp` |
| VB.NET | `vb` | `vb` |
| Python | `py`, `pyi`, `pyw` | `python` |
| Ruby | `rb`, `rake`, `gemspec` | `ruby` |
| PHP | `php`, `phtml` | `php` |
| Perl | `pl`, `pm` | `perl` |
| JavaScript | `js`, `mjs`, `cjs`, `jsx` | `javascript` |
| TypeScript | `ts`, `tsx`, `mts`, `cts` | `typescript` |
| Go | `go` | `go` (**dedicated** `compiler::go_front`) |
| V | `v` | `v` (**dedicated** `compiler::v_front`) |
| Rust | `rs` | `rust` (**dedicated** `compiler::rust_front`) |
| Zig | `zig` | `zig` |
| Dart | `dart` | `dart` |
| Lua | `lua` | `lua` |
| Clojure | `clj`, `cljs`, `cljc` | `clojure` |
| Groovy | `groovy` | `groovy` |
| Elixir | `ex`, `exs` | `elixir` |
| Erlang | `erl`, `hrl` | `erlang` |
| Haskell | `hs`, `lhs` | `haskell` |
| Julia | `jl` | `julia` |
| R | `r` | `r` |
| Nim | `nim` | `nim` |
| D | `d` | `d` |
| Crystal | `cr` | `crystal` |

**`.swift`** is **not** in this table: it selects the Swift SIL path (`swiftc` or `IN_NATIVE_SWIFT_SIL` subset), not Core IR.

**`.h` headers** map to **`c`**; some Objective-C headers share `.h` — ambiguous paths stay **`c`** Tree-sitter (`function_definition` extraction).

## Current compatibility ladder

| Level | Meaning | Current fronts |
|-------|---------|----------------|
| 0 | Routes to a known `ParserId`, but no compatible grammar/front is wired; callers get an `.icore` hint. | `clojure`, `nim`, `d`, `crystal`, `vb` |
| 1 | Extracts top-level declarations into `UnifiedModule`; bodies are empty or ignored. | `icore` v1, `kotlin`, `scala`, `csharp`, `fsharp`, `python`, `ruby`, `php`, `perl`, `javascript`, `typescript`, `zig`, `dart`, `lua`, `elixir`, `erlang`, `haskell`, `julia`, `r`; Objective-C methods are also declaration-only. |
| 2 | Lowers a bounded statement/expression subset into Core IR. | `.in`, `icore` v2, `rust`, `go`, `v`, `java`, `groovy`; C / C++ / Objective-C++ functions support only trivial `return <integer>;`, `return <param>;`, or `return;` bodies. |
| 3 | Typechecks enough language semantics to produce reliable diagnostics. | Not landed for a full language family yet. |
| 4 | Emits graph-aware SIL artifacts and agent repair plans. | Agent JSON exists; no language front is promoted to this level until its diagnostics and repair plans are source-semantic for that front. |
| 5 | Supports production build/hotreload semantics for that language family. | Swift uses the separate Swift SIL path today; Core IR language families are not at this level yet. |

The ladder is a routing and agent-contract signal, not a promise of full language semantics. `swift` is intentionally outside the Core IR extension table: it selects the Swift SIL path (`swiftc` and/or `IN_NATIVE_SWIFT_SIL`) until agent-mode JSON gives it a comparable compatibility report.

## Compiler roadmap (honest scope)

Implementing every OO / C-like language means, per language: lexer/grammar → AST → [`UnifiedModule`](multi-frontend-ir.md) (or **icore** JSON) → [`compiler::driver` / `lower_core`](../../in-cli/src/compiler/driver.rs) → textual SIL. That is **large, parallel work**; [general-compiler.md](general-compiler.md) is the umbrella roadmap. This document tracks **routing and names** so CI, agents, and contributors share one enum and extension table as fronts land.
