# Benchmarks

| Doc | What it measures |
|-----|------------------|
| [jit.md](jit.md) | JIT compile + run latency on macOS ARM64 |
| [polyglot-compilers.md](polyglot-compilers.md) | `in compile` vs installed native compilers on polyglot samples |
| [self-host-vs-native.md](self-host-vs-native.md) | `in build` vs **rustc** (time, binary size, startup) on `in-cli` |

Regenerate:

```bash
./scripts/bench-self-host.sh && python3 scripts/render-self-host-bench-md.py
./scripts/bench_polyglot_compilers.v   # polyglot matrix
```

Swift-vs-in benchmark removed; use **polyglot-compilers** for cross-language compile-time comparison.