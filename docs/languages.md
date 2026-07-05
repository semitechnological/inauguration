# Language fronts

**40** parser fronts route into one **Core IR** pipeline. Authoritative matrix:

```bash
in languages
in languages --json
```

## First-class

| Language | Extensions | Front | Capabilities |
|----------|------------|-------|----------------|
| **inlang** | `.in` | `in_lang_parse` | parse, lower, typecheck, boundary |
| **icore** | `.icore` | `compiler::icore` | parse, lower, typecheck, boundary |
| **Rust** | `.rs` | `compiler::rust_front` | parse, lower, typecheck, boundary |
| **V** | `.v` | `compiler::tree_front` | parse, lower, typecheck, boundary |

## Tree-sitter polyglot (selection)

Swift, Go, C, C++, Objective-C, Java, JavaScript, TypeScript, Python, Ruby, Zig, PHP, Kotlin, Scala, C#, Nim, D, Haskell, OCaml, Lua, and more—see `in languages --json` for the full list, example paths, and maturity gates.

Resolution: extension map, `IN_PARSER`, `#!in parser=…` — [parser-surface.md](parser-surface.md).

## Runtime boundary

Owned path is **JIT / native_emit** on AArch64 and x86_64. External toolchains (`rustc`, `swiftc`, …) are optional parity checks in `in test --external-parity`, not the default execution engine.