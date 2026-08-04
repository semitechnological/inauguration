# inauguration

Rust CLI for the inauguration Swift toolchain experiments: embedded hybrid
pipeline (AST refresh → frontend → SIL analysis wave), hotreload daemon (Unix
sockets), SwiftPM staging under `.build/bin` and `.build/artifacts`, plugins,
and workspace-only integration commands (`in test`).

## Install

```bash
cargo install inauguration
```

Build with `--features extended` when the V frontend (and the other optional
Tree-sitter fronts) is needed:

```bash
cargo install inauguration --features extended
```

Full repository layout, benchmarks, Wax/Homebrew installers, and contribution
docs live in the GitHub repo.

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
- **Script runner**: `in eval <file>` is the fastest way to run an `.in`, `.rs`,
  `.zig`, `.go`, `.v`, or `.poly` script through the JIT. The V frontend is
  included in builds with `--features extended`; `in execute` and `in build`
  are available for explicit compile/artifact paths.
- **Bytecode VM**: retired. `in eval`, `in execute`, and `in build` use the
  JIT-only path.

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
|----------|------:|---------------:|-------:|
| `fib(30)` JIT build | ~35 ms | <1 ms | ~0.3 ms |
| `fib(35)` JIT runtime | ~55 ms | ~55 ms | ~55 ms |
| Self-host parse (1965 fns) | ~175 ms | ~175 ms | resident |
| Space kernel compile | ~50 ms | ~30 ms | resident |
| Polyglot sample compile | ~1.9 s | ~0.5 s | ~0.5 s |

### Performance notes

- Cold times are dominated by the `in` binary's process startup (~30 ms), not by
  the compiler pipeline. The actual parse/lower/emit for small programs is under
  2 ms. `in daemon start` removes that startup for repeated eval/build calls.
- The JIT is the default and only execution path; the bytecode VM is retired.
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
