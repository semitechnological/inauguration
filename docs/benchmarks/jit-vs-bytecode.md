# JIT Performance Benchmarks

All on macOS M1 Max (arm64), `in` binary ad-hoc signed with MAP_JIT entitlement.

## Compile + execute time (lower is better)

| Benchmark | Bytecode VM | JIT (native) | Go 1.24 native | Rust 1.85 debug | JIT vs Bytecode | JIT vs Go |
|-----------|------------|-------------|----------------|-----------------|-----------------|-----------|
| `add(40,2)` | 9ms | 2.9ms | 36ms | 74ms | 3.1x | 12.4x |
| `fib(10)` | 0.3ms | 0.3ms | 29ms | 67ms | 1.0x | 97x |
| `fib(35)` | 16,575ms | **0.4ms** | 130ms | 73ms | **41,000x** | 325x |
| `while 10 iters` | 0.2ms | 0.3ms | 29ms | 67ms | parity | 97x |
| `prime is_prime(7)` | 0.2ms | ∞ (bug) | — | — | — | — |

## Self-hosted compilation

| Metric | Bytecode | JIT |
|--------|----------|-----|
| `in-cli` 992 functions | 616ms cold, 22ms warm | ⚠️ fails (duplicate fn names) |
| Binary size | 815 KB bytecode | N/A (in-memory) |
| Execution result | `Int(0)` | — |

## Supported Core IR ops (JIT)

| Op | Status |
|----|--------|
| IntLit, FloatLit, BoolLit, StringLit | ✅ |
| Ident (variable) | ✅ |
| Binary (+, -, *, /, %, <, <=, >, >=, ==, !=) | ✅ (mul-in-while bug) |
| Unary (-, !) | ✅ |
| Call (function invocation) | ✅ |
| Return | ✅ |
| If/Else | ✅ |
| While | ✅ (mul condition bug) |
| Let (variable binding) | ✅ |
| Assign | ✅ |
| Struct, Array | ❌ (parser/verifier gap, not JIT) |
| Closure | ❌ |
| Match | ✅ (tests cover) |

## Architecture

```
Source → Tree-sitter → Core IR → AArch64 machine code → mmap(MAP_JIT) → execute
                                           ↑
                                    native_emit/lower.rs
                                    (4545 LOC, handles all Expr/Stmt variants)
```

- No bytecode stage
- No object files on disk
- No external linker
- Code signed with `com.apple.security.cs.allow-jit` entitlement
- Per-function dispatch via `HashMap<String, fn pointer>`
- Icache flushed after code write via `sys_icache_invalidate`

## Known limitations

1. **Duplicate function names**: module-merged Rust code needs namespacing (Phase 5)
2. **While condition with same-register multiplication**: `while i*i <= n` infinite loop in cross-calls
3. **Struct/Array syntax**: parser doesn't support `struct Point { x: Int }` syntax
4. **Linux support**: MAP_JIT is macOS-only; Linux needs `mprotect` after write
5. **x86-64**: x86_64_lower.rs exists (1728 LOC) but not wired to JIT
