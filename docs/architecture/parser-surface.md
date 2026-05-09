# Parser surface (`in build`)

`in-cli/src/parser_registry.rs` resolves which **front** runs before the hybrid SIL pipeline. Only **`.in`** parses into [`UnifiedModule`](multi-frontend-ir.md) today; every other tracked language is a **stub** (clear error, no `swiftc`).

Resolution order is documented in the `parser_registry` module rustdoc. Summary:

1. `--parser in` → `.in` front  
2. Magic first line `#!in parser=…` on a regular file  
3. `IN_PARSER=in`  
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

Implementing “all” OO and C-like languages means, per language: lexer/grammar → AST → [`UnifiedModule`](multi-frontend-ir.md) or an extended IR → [`lower_core`](../in-cli/src/lower_core.rs) → textual SIL `hybrid_sil` accepts. That is **large, parallel work** (see [native-swift-master-plan.md](native-swift-master-plan.md) for the Swift-shaped slice). This document tracks **routing and names** so CI, agents, and contributors share one enum and extension table as fronts land.
