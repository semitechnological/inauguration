# Self-host vs native benchmarks

Measured on macOS ARM64 (M3), `in` v0.7.1.

## Self-host parse

| Metric | `in build --path in-cli/src/main.rs` |
|--------|--------------------------------------:|
| Functions parsed | 1,965 |
| Functions typed | 1,965 |
| Call edges | 4,815 |
| Wall time | ~175 ms |
| Backend | owned Core IR path (no `--out`) |

A full native self-build (`--out /tmp/in`) is currently blocked by stdlib surface
coverage; the parser/typechecker path through the owned Core IR backend exercises
the full front end.

## Language coverage

| Language | Compile | Execute | Notes |
|----------|:-------:|:-------:|-------|
| `.in` | ✅ | ✅ | full language subset, JIT |
| `.icore` | ✅ | ✅ | direct Core IR, JIT |
| `.rs` (simple) | ✅ | ✅ | native Mach-O via as+ld |
| `.rs` (self-host) | ✅ | ⚠️ | parses/types; native blocked by stdlib |
| `.zig` | ✅ | ✅ | simple functions |
| `.go` | ✅ | ✅ | answer example |
| `.swift` | ✅ | ✅ | with `--features extended` |
| `.poly` | ✅ | ✅ | polyglot eval |

## Optimization opportunities

1. **Process startup** — `in` cold invocations spend ~30 ms in binary load. Use
   `in daemon start` to keep the compiler resident for repeated self-host checks.
2. **Rust front** — parallel per-file parsing of external modules is now enabled;
   next step is incremental re-parse from the Core IR cache.
3. **Compile cache** — source-hash caching works; next step is to cache the
   lowered native code and avoid re-linking inrt builtins.
4. **Stdlib surface** — native self-host needs thin C-ABI wrappers for the
   remaining stdlib calls instead of resolving them through the Rust standard
   library.
