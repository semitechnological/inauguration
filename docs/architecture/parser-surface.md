# Parser surface (`in build`)

`in-cli/src/parser_registry.rs` resolves which **front** runs before the hybrid SIL pipeline. **Implemented** Core IR fronts: **`.in`**, **`.icore` (JSON)**. Every other tracked extension is a **stub** (clear error, no `swiftc`) until a parser lands — see [general-compiler.md](general-compiler.md).

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

## Extension → front (stubs unless noted)

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
| Go | `go` | `go` |
| Rust | `rs` | `rust` |
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

**`.h` headers** map to **`c`**; some Objective-C headers share `.h` — today that is ambiguous and defaults to the C-like stub.

## Compiler roadmap (honest scope)

Implementing every OO / C-like language means, per language: lexer/grammar → AST → [`UnifiedModule`](multi-frontend-ir.md) (or **icore** JSON) → [`compiler::driver` / `lower_core`](../in-cli/src/compiler/driver.rs) → textual SIL. That is **large, parallel work**; [general-compiler.md](general-compiler.md) is the umbrella roadmap. This document tracks **routing and names** so CI, agents, and contributors share one enum and extension table as fronts land.
