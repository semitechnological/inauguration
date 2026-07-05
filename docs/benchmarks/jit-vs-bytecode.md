# JIT vs bytecode benchmarks

Measured on macOS ARM64 (M3), `in` v0.7.1.

## Compile time (cold)

| Benchmark | Bytecode | JIT (native) |
|-----------|----------:|-------------:|
| `add(40,2)` | ~1 ms | ~35 ms |
| `fib(30)` | ~1 ms | ~35 ms |
| `fib(30)` warm (cached) | ~0.1 ms | ~0.1 ms |

Cold JIT times are dominated by the `in` binary's process startup (~30 ms). The
actual parse + lower + emit for small workloads is under 2 ms.

## Execution time

| Benchmark | Bytecode VM | JIT (native) |
|-----------|----------:|-------------:|
| `fib(30)` | ~1,600 ms | ~1 ms |

The bytecode VM is stack-based and interpretive; the JIT path emits AArch64
machine code and is the intended default for hot loops.

## Status

| Op | JIT | Bytecode |
|----|-----|----------|
| IntLit, FloatLit, BoolLit, StringLit | ✅ | ✅ |
| Ident, Binary, Unary | ✅ | ✅ |
| Call, Return, If/Else, While, Let, Assign | ✅ | ✅ |
| Match | ✅ | ✅ |
| Struct, Array | ✅ | ✅ (native subset) |
| Closure | ❌ | ❌ |

## Architecture

```
Source → parser → Core IR → native_emit/lower → AArch64/x86_64 machine code
                                                    ↓
                                              mmap(MAP_JIT) / boot image
```

- No LLVM, no external linker on the JIT path
- x86_64 lowering is used for freestanding boot images
- I-cache flushed via `sys_icache_invalidate` on Apple Silicon

## Optimization opportunities

1. **Process startup** — the largest cold-time cost. A persistent compiler daemon
   would drop small-program compiles from ~35 ms to ~1 ms.
2. **Bytecode VM** — switch to a register-based VM or add a hot-path JIT tier to
   close the 1000x+ execution gap with native code.
3. **Native lowering** — cache lowered function bodies when the source hash and
   target triple haven't changed.
