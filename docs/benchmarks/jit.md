# JIT benchmarks

Measured on macOS ARM64 (M3), `in` v0.7.1.

## Compile time

| Benchmark | Cold (binary) | Warm (cached) | Daemon |
|-----------|--------------:|--------------:|-------:|
| `add(40,2)` | ~35 ms | ~0.1 ms | ~0.3 ms |
| `fib(30)` | ~35 ms | ~0.1 ms | ~0.3 ms |
| Polyglot sample (38 files) | ~1.9 s | ~0.5 s | ~0.5 s |

Cold times are dominated by the `in` binary's process startup (~30 ms). The
actual parse + lower + emit for small workloads is under 2 ms. `in daemon start`
removes that startup cost for repeated eval/build calls.

## Execution time

| Benchmark | JIT (native) |
|-----------|-------------:|
| `fib(30)` | ~1 ms |
| `fib(35)` | ~55 ms |

The JIT path emits AArch64 machine code and is the default execution path.

## Status

| Op | JIT |
|----|-----|
| IntLit, FloatLit, BoolLit, StringLit | ✅ |
| Ident, Binary, Unary | ✅ |
| Call, Return, If/Else, While, Let, Assign | ✅ |
| Match | ✅ |
| Struct, Array | ✅ (native subset) |
| Closure | ❌ |

## Architecture

```
Source → parser → Core IR → native_emit/lower → AArch64/x86_64 machine code
                                                    ↓
                                              mmap(MAP_JIT) / boot image
```

- No LLVM, no external linker on the JIT path
- No bytecode VM; JIT is the only execution path
- x86_64 lowering is used for freestanding boot images
- I-cache flushed via `sys_icache_invalidate` on Apple Silicon

## Optimization opportunities

1. **Native lowering** — cache lowered function bodies when the source hash and
   target triple haven't changed.
2. **Incremental parse** — reuse the Core IR cache for unchanged imports.
3. **Function-level parallel lowering** — lower independent functions in parallel.
