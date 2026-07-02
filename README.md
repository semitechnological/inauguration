# inauguration

One **Core IR** → **native_emit/JIT** pipeline, 40 language frontends.
Own compiler backend — no LLVM, no bytecode VM, no SIL.

## What it is

`inauguration` is a self-hosted compiler project that compiles its own Rust source
code to native binaries without delegating to cargo/rustc. It uses its own
AArch64/x86_64 native backend with an `as`+`ld` pipeline.

## Current status

**Self-hosting in progress.** `in` can compile its own Rust source to a working
Mach-O binary. The binary runs correctly for:
- Arithmetic, functions, recursion (fib(30)=832040 ✓)
- Struct allocations, field access, method dispatch (Point.magnitude ✓)
- While/for/loop control flow, if/else, match
- `_exit` via libSystem linking

**Remaining**: 120+ unresolved std library symbols. These are being resolved
by writing thin Rust wrappers around the C ABI (libSystem on macOS), compiled
directly by `in`.

## Unified stdlib architecture

One portable systems-level stdlib for ALL languages. Every language's stdlib
surface maps to the same underlying implementations:

```
Rust: std::fs::read_dir  \
Python: os.listdir       |  →  Core IR: StdCall("fs.read_dir")
Go: os.ReadDir          /       →  inrt: fs_read_dir(C ABI)
                               →  syscall: openat + getdents64
```

### How it works

1. **Language front** (Tree-sitter or native) parses source to Core IR
2. **Stdlib mapper** recognizes function calls against known stdlib surfaces:
   - Rust `std::fs::*`, `std::env::*`, `std::path::*`, etc.
   - Python `os.*`, `sys.*`, `pathlib.*`, etc.
   - Go `os.*`, `fmt.*`, `net.*`, etc.
   - C `fopen`, `readdir`, `getenv`, etc.
   - JavaScript `fs.*`, `path.*`, `process.*`, etc.
   - PyPI/cargo/npm packages that transitively use stdlib
3. Each recognized call is redirected to **inrt** — our unified runtime
4. inrt implements each function as a portable C ABI wrapper:
   - macOS: calls libSystem
   - Linux: calls glibc/musl
   - No platform: direct syscall in AArch64/x86_64 assembly

### Layer stack

```
┌─────────────────────────────────────────────────┐
│  Language frontends (Rust, Python, Go, C, ...)   │
│  Each produces Core IR with stdlib annotations   │
├─────────────────────────────────────────────────┤
│  Stdlib mapper (language-agnostic)              │
│  {lang}.{module}.{func} → inrt.{func}           │
├─────────────────────────────────────────────────┤
│  inrt — unified runtime                         │
│  Compiled by `in` from Rust source               │
│  Cross-platform: portable C ABI + direct syscall │
├─────────────────────────────────────────────────┤
│  Platform layer                                 │
│  macOS: libSystem.dylib  Linux: libc.so         │
│  Fallback: inline syscall (AArch64/x86_64)      │
└─────────────────────────────────────────────────┘
```

### Cross-platform support

| Platform | C ABI lib | Fallback | Status |
|----------|-----------|----------|--------|
| macOS (arm64) | libSystem.dylib | Mach syscall | **working** |
| Linux (x86_64) | glibc/musl | Linux syscall | planned |
| Linux (arm64) | glibc/musl | Linux syscall | planned |
| Windows (x86_64) | msvcrt | NT syscall | planned |
| Chimera Linux (musl) | musl | Linux syscall | planned |

### Why not compile Rust stdlib

The Rust standard library is 3M+ lines using every Rust feature (closures,
generics, async, proc macros, const eval, inline asm). Compiling it from
source requires implementing all of those features first. Instead, each std
function `in` needs becomes a thin Rust `extern "C"` wrapper that:

1. Is simple enough for `in`'s backend to compile (no generics, no traits)
2. Calls the C ABI (libSystem, glibc) for the actual system work
3. Is portable — the C ABI is the universal system interface

This gives us **immediate cross-platform support** with minimal backend work.

## Install

```bash
wax install inauguration        # macOS (Homebrew-based)
cargo install inauguration      # crates.io (all platforms)
./install.sh                    # from source
```

Binary: **8.7MB** (release, LTO+strip), no LLVM dependency.

## Language Support

39 Tree-sitter parsers + native `.in`/`.icore` frontends → one Core IR.

| Feature | Status |
|---------|--------|
| `.in` language (canonical + .icore) | parse, typecheck, native/JIT |
| Rust (simple functions) | parse, native binary (as+ld) |
| Rust (structs, methods, dispatch) | parse, native binary |
| Rust (closures, generics, traits) | parse only |
| Rust (proc macros, async, unsafe) | parse skipped |
| 37 other languages (Swift, Go, Zig, etc.) | Tree-sitter parse + IR |
| Native AArch64 backend | JIT + Mach-O binary |
| Native x86_64 backend | JIT only |
| MIR (Machine IR) layer | offset-deferred assembly |
| Self-hosting (in compiles in) | **in progress** |

## Compile-time performance

| Workload | Time | Notes |
|----------|------|-------|
| fib(30) JIT | ~30ms | 2 functions, in-memory JIT |
| fib(30) native binary | ~100ms | as+ld pipeline |
| Self-host parse (3444 fns) | ~50ms | Rust front → Core IR |
| Self-host native build | ~2.5s | as+ld + cargo metadata |
| Rust stdlib compile | N/A | Using syscall wrappers instead |

## Repository layout

| Path | What |
|------|------|
| `in-cli/src/` | CLI, parsers, Core IR, native emit, JIT, inrt |
| `in-cli/src/compiler/` | Rust front, Tree-sitter front, pipeline |
| `in-cli/src/native_emit/` | AArch64/x86_64 lowering, assembly emit, Mach-O writer |
| `in-cli/src/inrt.rs` | Builtin runtime — `.in` language stdlib |
| `plugins/registry/` | Project accelerators |
| `scripts/` | Validation, benchmarks, install |

## Self-hosting

```bash
# Stage 0: build with cargo
cargo build --release --bin in --features extended

# Stage 1: build with itself (native backend)
./target/release/in build --path in-cli/src/main.rs --out /tmp/in

# Stage 2: verify bootstrap
/tmp/in build --path in-cli/src/main.rs --out /tmp/in2
```

The native backend produces Mach-O 64-bit arm64 executables via `as`+`ld`,
linking against libSystem for C ABI functions (`_exit`, etc.).

## License

MPL-2.0
