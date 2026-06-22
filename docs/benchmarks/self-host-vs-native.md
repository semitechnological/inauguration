# inauguration compiler · benchmark comparison

## Self-hosted vs Native (cargo)

| Metric | Self-hosted (`in eval in-cli`) | Native (`cargo build --release`) |
|--------|-------------------------------|----------------------------------|
| Compile time | 705ms | 85ms (incremental) |
| Binary size | 815 KB bytecode | 73 MB native |
| Functions parsed | 985 | N/A (rustc internal) |
| Bytecode functions | 184 (after DCE) | N/A |
| SIL size | 211 KB | N/A |
| Execution result | Int(0) | N/A (binary runs) |

## Language coverage

| Language | Compile | Execute | Notes |
|----------|---------|---------|-------|
| `.in` | ✅ 7 examples | ✅ All correct | Full language: while, for, if/else, fn, recursion |
| `.rs` (simple) | ✅ | ✅ | add_multiply.rs → Int(60) |
| `.rs` (self-host) | ✅ 985 funcs | ✅ Int(0) | Cargo.toml reading + dep linking |
| `.zig` | ✅ | ✅ | add(40,2) → Int(42) |
| `.go` | ❌ | ❌ | Entry point resolution pending |
| `.swift` | ⚠️ | ❌ | Parses but parameter resolution fails |
| `.poly` (polyglot) | ✅ | ✅ | 4-language IO, 5-language compute |

## Compiler pipeline

```
in eval file.in          # compile → verify → typecheck → bytecode → execute
in eval file.rs          # compile → cargo-dep-link → verify-skip → bytecode → execute
in eval file.zig         # compile → verify → bytecode → execute
in eval file.poly        # polyglot eval (multi-language expressions)
```

## Native runtime (external calls)

70+ stdlib/crate functions implemented as native callbacks:
- File system: `cwd`, `join`, `is_dir`, `exists`, `read_to_string`, `create_dir_all`
- Path: `extension`, `file_stem`, `parent`, `display`, `to_string_lossy`
- String: `String::new`, `format`, `to_string`, `clone`
- Result/Option: `unwrap`, `is_ok`, `is_err`, `map_err`, `unwrap_or`
- Serde: `serde_json::to_string_pretty`
- Process: `std::process::exit`, `std::env::var`, `std::env::temp_dir`

## Optimization opportunities

1. **Cargo metadata caching** — `cargo metadata` takes ~500ms per compilation
2. **Bytecode DCE** — 985 parsed → 184 functions (82% reduction already)
3. **Parallel compilation** — dependencies compiled sequentially
4. **Incremental compilation** — no module-level caching yet
5. **Binary size** — 815KB bytecode vs 73MB native (90x smaller)
