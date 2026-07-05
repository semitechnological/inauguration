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
- **Compiler daemon**: `in daemon start` keeps the compiler resident; repeated
  `in eval` / `in build` calls skip the binary startup cost.
- **Self-hosting**: `in build --path in-cli/src/main.rs` parses and type-checks
  the Rust source (1965 functions) via the owned Core IR path. A full native
  self-build is still blocked by stdlib surface coverage; see the language support
  table below.
- **Bytecode VM**: retired from the default path. `in eval` and `in build` use
  the JIT. The bytecode backend remains available via `in compile --target
  bytecode` and `in execute-bytecode` for the conformance suite and legacy tests.

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
| `.in` / `.icore` | parse, typecheck, native/JIT |
| Rust (simple functions, structs, methods) | parse, native binary (as+ld) |
| Rust (closures, generics, traits, async, unsafe) | parse only / skipped |
| 37 other languages (Swift, Go, Zig, etc.) | Tree-sitter parse → Core IR |
| Native AArch64 backend | JIT + Mach-O binary |
| Native x86_64 backend | JIT + freestanding boot image |
| MIR layer | offset-deferred assembly |
| Compiler daemon | Unix socket server, eval/build path |
| Self-hosting (native) | in progress |

## Performance

Measured on macOS ARM64 (M3), `in` v0.7.1.

| Workload | Cold | Warm (cached) | Daemon |
|----------|------:|---------------:|-------:|-------|
| `fib(30)` JIT build | ~35 ms | <1 ms | ~0.3 ms |
| `fib(35)` JIT runtime | ~55 ms | ~55 ms | ~55 ms |
| `fib(35)` bytecode runtime | ~16 s | — | — |
| Self-host parse (1965 fns) | ~175 ms | ~175 ms | resident |
| Space kernel compile | ~50 ms | ~30 ms | resident |
| Polyglot sample compile | ~1.9 s | ~0.5 s | ~0.5 s |

### Performance notes

- Cold times are dominated by the `in` binary's process startup (~30 ms), not by
  the compiler pipeline. The actual parse/lower/emit for small programs is under
  2 ms. `in daemon start` removes that startup for repeated eval/build calls.
- The JIT is the default execution path; the bytecode VM is being retired and is
  no longer used for hot loops.
- `.in` imports and external Rust modules are parsed in parallel.
- AArch64 function lowering is now parallel (one thread per function).
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
