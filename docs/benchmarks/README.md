# Benchmarks

| Doc | What it measures |
|-----|------------------|
| [jit.md](jit.md) | JIT compile + run latency on macOS ARM64 |
| [polyglot-compilers.md](polyglot-compilers.md) | `in compile` vs installed native compilers on polyglot samples |
| [self-host-vs-native.md](self-host-vs-native.md) | Self-host parse/compile vs native artifact path |

Regenerate polyglot numbers:

```bash
./scripts/bench_polyglot_compilers.v   # or project bench script
```

Swift-vs-in benchmark removed; use **polyglot-compilers** for cross-language compile-time comparison.