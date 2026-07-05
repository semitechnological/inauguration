# Self-host vs native benchmarks

Measured on macOS ARM64 (M3), `in` v0.7.1.

## Self-host parse

| Metric | `in build --path in-cli/src/main.rs` |
|--------|--------------------------------------:|
| Functions parsed | 1,965 |
| Functions typed | 1,965 |
| Call edges | 4,815 |
| Wall time | ~175 ms |
| Backend | bytecode path (no `--out`) |

A full native self-build (`--out /tmp/in`) is currently blocked by stdlib surface
coverage; the parser/typechecker path through the bytecode backend exercises the
full front end.

## Language coverage

| Language | Compile | Execute | Notes |
|----------|:-------:|:-------:|-------|
| `.in` | ✅ | ✅ | full language subset |
| `.icore` | ✅ | ✅ | direct Core IR |
| `.rs` (simple) | ✅ | ✅ | native Mach-O via as+ld |
| `.rs` (self-host) | ✅ | ⚠️ | parses/types; native blocked by stdlib |
| `.zig` | ✅ | ✅ | simple functions |
| `.go` | ✅ | ✅ | answer example |
| `.swift` | ✅ | ✅ | with `--features extended` |
| `.poly` | ✅ | ✅ | polyglot eval |

## Optimization opportunities

1. **Process startup** — `in` cold invocations spend ~30 ms in binary load. A
   compiler daemon would make repeated self-host checks nearly instant.
2. **Rust front** — single-threaded parsing of 1,965 functions. Parallel
   per-file parsing and incremental re-parse from the Core IR cache would cut
   self-host time.
3. **Compile cache** — source-hash caching works; next step is to cache the
   lowered native code and avoid re-linking inrt builtins.
4. **Stdlib surface** — native self-host needs thin C-ABI wrappers for the
   remaining stdlib calls instead of resolving them through the Rust standard
   library.
