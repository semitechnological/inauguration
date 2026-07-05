# inauguration

One **Core IR** → **native_emit/JIT** pipeline, 40 language frontends.
Own compiler backend — no LLVM, no bytecode VM, no SIL.

## What it is

`inauguration` is a compiler toolchain that lowers multiple source languages
into a shared Core IR and then compiles that IR to native machine code or runs
it in a JIT. It targets its own backends for AArch64 and x86_64.

## Current status

- **JIT execution**: `.in`, `.icore`, and many polyglot samples run through the
  owned JIT on macOS and Linux.
- **Native binaries**: Mach-O arm64 executables via `as`+`ld` on macOS; static
  libs / boot images for x86_64 freestanding.
- **Self-hosting**: `in build --path in-cli/src/main.rs` parses and type-checks
  the Rust source (1965 functions) via the bytecode path. A full native
  self-build is still blocked by stdlib surface coverage; see the language support
  table below.

## Unified stdlib

One portable systems-level stdlib backs all language fronts. Calls like
`std::fs::read_dir`, `os.listdir`, or `os.ReadDir` lower to the same Core IR
stdlib call, then to `inrt` C-ABI wrappers, then to libSystem/glibc or direct
syscalls.

## Install

```bash
wax install inauguration        # macOS (Homebrew-based)
cargo install inauguration      # crates.io (all platforms)
./install.sh                    # from source
```

Binary: ~8.7MB (release, LTO+strip), no LLVM dependency.

## Language support

| Feature | Status |
|---------|--------|
| `.in` / `.icore` | parse, typecheck, native/JIT, bytecode |
| Rust (simple functions, structs, methods) | parse, native binary (as+ld) |
| Rust (closures, generics, traits, async, unsafe) | parse only / skipped |
| 37 other languages (Swift, Go, Zig, etc.) | Tree-sitter parse → Core IR |
| Native AArch64 backend | JIT + Mach-O binary |
| Native x86_64 backend | JIT + freestanding boot image |
| MIR layer | offset-deferred assembly |
| Self-hosting (native) | in progress |

## Performance

Measured on macOS ARM64 (M3), `in` v0.7.1.

| Workload | Cold | Warm (cached) | Notes |
|----------|------:|---------------:|-------|
| fib(30) JIT build | ~35 ms | <1 ms | process startup dominates cold |
| fib(30) bytecode compile | ~1 ms | ~0.1 ms | parse + lower + emit |
| Self-host parse (1965 fns) | ~175 ms | ~175 ms | Rust front → Core IR |
| Space kernel compile | ~50 ms | ~30 ms | x86_64 boot image |
| fib(30) bytecode runtime | ~1.6 s | — | bytecode VM, not JIT |

### Performance notes

- Cold times are dominated by the `in` binary's process startup (~30 ms), not by
  the compiler pipeline. The actual parse/lower/emit for small programs is under
  2 ms. A compiler daemon or resident LSP-style server would erase that startup.
- The bytecode VM is currently the slowest path for compute-heavy code; the JIT
  path is the intended default for hot loops. A register-based VM or a tier that
  JITs hot bytecode paths would close the gap.
- The Rust front is single-threaded and re-parses the entire module. Parallel
  per-file parsing and incremental re-parse from the Core IR cache would speed up
  self-host-style workloads.
- The compile cache already works by source hash; the next win is avoiding
  re-linking the runtime builtins when the source hasn't changed.

## Repository layout

| Path | What |
|------|------|
| `in-cli/src/` | CLI, parsers, Core IR, native emit, JIT, inrt |
| `in-cli/src/compiler/` | Rust front, Tree-sitter front, pipeline |
| `in-cli/src/native_emit/` | AArch64/x86_64 lowering, assembly emit, Mach-O writer |
| `in-cli/src/inrt.rs` | Builtin runtime — `.in` language stdlib |
| `plugins/registry/` | Project accelerators |
| `scripts/` | Validation, benchmarks, install |

## License

MPL-2.0
