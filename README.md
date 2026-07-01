# inauguration

`inauguration` is a language and general compiler project. One **Core IR** → **native_emit/JIT** pipeline, 40 language frontends.

- **`.in` language**: capability-aware systems and orchestration language with canonical and human-friendly forms.
- **General compiler pipeline**: Tree-sitter polyglot frontends → Core IR → MIR → native_emit/JIT. No LLVM, no bytecode VM.
- **`in` CLI**: build, inspect, graph, package, test, run, and developer workflow.
- **Agent-readable facts**: JSON reports for parser decisions, imports, capabilities, effects, call graphs, diagnostics, and timing.
- **MIR layer**: Machine IR between Core IR and native emit for relocatable JIT code
- **JIT-primary pipeline**: Core IR → MIR → native_emit/JIT dispatch. No LLVM, no bytecode VM, no linker.

Inauguration owns the language, compiler infrastructure, Core IR, backend contracts, orchestration facts, and runtime-boundary tooling. UI rendering belongs in sibling projects such as Crepuscularity.

## Install

```bash
# Wax (macOS, built on Homebrew)
wax install inauguration

# crates.io (all platforms)
cargo install inauguration

# From source
git clone https://github.com/tschk/inauguration.git
cd inauguration
./install.sh
```

Binary size: **8.7MB** (release, LTO+strip), self-contained, no LLVM dependency.



## Adding Packages

`in add` pulls dependencies from crates.io, npm, PyPI, or Go modules into your project. Declared in `inauguration.package`, locked in `inauguration.lock`.

```bash
in add cargo:crepuscularity
in add npm:hono --version 4.12.25
in add pypi:flask
in add go:fiber --source github.com/gofiber/fiber/v2
```

`inauguration.package` example:

```yaml
name: my-project
version: 0.1.0
dependencies:
  cargo:crepuscularity:
    version: latest
    kind: cargo
  npm:hono:
    version: 4.12.25
    kind: npm
  pypi:flask:
    version: latest
    kind: pypi
  go:fiber:
    version: latest
    kind: go
    source: github.com/gofiber/fiber/v2
```

Supported ecosystems: `cargo:` (crates.io), `npm:` (npm registry), `pypi:` (PyPI), `go:` (Go modules).

## Repository Layout

| Directory | What |
|-----------|------|
| `in-cli` | CLI, `.in` parser, Core IR, MIR, compile reporting, graph/package commands, JIT/native backend, hotreload daemon, protocol gen |
| `compiler\/rust-driver` | orchestration pipeline, stage model, IR analysis, batch path |
| `plugins/registry` | project accelerators (aurorality) |
| `docs/architecture` | language, compiler, interop, roadmap docs |
| `scripts` | validation, generation, install, workflow |

## Language Support

39 Tree-sitter parsers + native `.in`/`.icore` frontends, all sharing one Core IR.

| Language | parse | lower | typecheck | boundary | native/JIT |
|----------|:-----:|:-----:|:---------:|:--------:|:----------:|
| in | ✓ | ✓ | ✓ | ✓ | ✓ |
| icore | ✓ | ✓ | ✓ | ✓ | ✓ |
| Rust | ✓ | ✓ | ✓ | ✓ | ✓ |
| Swift | ✓ | ✓ | ✓ | — | — |
| Go | ✓ | ✓ | ✓ | — | — |
| V | ✓ | ✓ | ✓ | ✓ | ✓ |
| C / C++ | ✓ | ✓ | ✓ | — | — |
| Objective-C / ObjC++ | ✓ | ✓ | ✓* | — | — |
| Java | ✓ | ✓ | ✓ | — | — |
| Groovy | ✓ | ✓ | ✓ | — | — |
| JavaScript | ✓ | ✓ | ✓ | ✓ | ✓ |
| TypeScript | ✓ | ✓ | ✓ | ✓ | ✓ |
| Kotlin | ✓ | ✓ | ✓ | — | — |
| Scala | ✓ | ✓ | ✓ | — | — |
| C# | ✓ | ✓ | ✓ | — | — |
| F# | ✓ | ✓ | — | — | — |
| VB.NET | ✓ | ✓ | ✓ | ✓ | — |
| Python | ✓ | ✓ | ✓ | — | — |
| Ruby | ✓ | ✓ | ✓ | — | — |
| PHP | ✓ | ✓ | ✓ | — | — |
| Perl | ✓ | ✓ | — | — | — |
| Zig | ✓ | ✓ | ✓ | ✓ | — |
| Dart | ✓ | ✓ | ✓ | — | — |
| Lua | ✓ | ✓ | ✓ | — | — |
| Clojure | ✓ | ✓ | ✓ | ✓ | — |
| Elixir | ✓ | ✓ | — | — | — |
| Erlang | ✓ | ✓ | — | — | — |
| Haskell | ✓ | ✓ | — | — | — |
| Nim | ✓ | ✓ | ✓ | ✓ | — |
| OCaml | ✓ | ✓ | ✓ | — | — |
| Julia | ✓ | ✓ | — | — | — |
| R | ✓ | ✓ | — | — | — |
| D | ✓ | ✓ | ✓ | ✓ | — |
| Crystal | ✓ | ✓ | ✓ | ✓ | — |
| Odin | ✓ | ✓ | ✓ | ✓ | ✓ |
| Hare | ✓ | ✓ | ✓ | ✓ | — |
| HolyC | ✓ | ✓ | — | — | — |

*\* Objective-C: typecheck partial.*

## Performance

Pipeline: **Source → UnifiedModule → native_emit/JIT** (Core IR verifier skipped for JIT).

### Compile time

| Workload | in (JIT, cold) | in + cargo (debug) | rustc (debug) |
|----------|:--------------:|:------------------:|:-------------:|
| `fn main() -> i64 { return 42 }` | **~30ms** | — | ~50ms |
| fib(30) (2 functions) | **~30ms** | — | ~50ms |
| Self-host: `in-cli/src/main.rs` (3444 functions) | **~12s** (analysis) | **~120ms** (parse 50ms + cargo 70ms) | **~2s** (incremental) |
| Self-host release build | — | **~42s** (parse 43ms + cargo --release) | **~43s** |

For self-hosting: `in build --path in-cli/src/main.rs --out /tmp/in --verbose`
- Frontend parses all 3444 functions in ~50ms (rust_front → Core IR)
- Cargo backend produces the native binary in ~70ms (incremental) or ~42s (full release)
- The output binary is a fully working `in` compiler that can compile itself again

### Compiler binary size

| Compiler | Size | Stripped | Dependencies |
|----------|------|----------|--------------|
| **in v0.7.1** (release, LTO optional) | **9.2MB** | — | self-contained, no LLVM |
| **in debug** | **26.7MB** | — | self-contained, no LLVM |
| Zig 0.16.0 | 20.9MB | — | LLVM backend |
| Go 1.26.4 | 13.8MB | — | self-contained |
| V 0.5.1 | 3.9MB | — | self-contained |
| Bun 1.x | 60.2MB | — | JS runtime + JSC |
| rustc (driver only) | 0.4MB | — | +LLVM dylibs (~200MB) |

Note: rustc is a thin driver; the actual LLVM codegen is in shared libraries.
in bundles no LLVM dependency — native emit is built-in.

### Benchmark suite (`cargo bench`)

| Benchmark | Time |
|-----------|------|
| `parse_textual_sil` / representative | 50.3 µs |
| `remove_debug_insts` / representative | 25.9 µs |
| `extract_call_graph` / representative | 26.2 µs |
| `extract_call_graph` / multi_function | 6.8 µs |
| `core_opt_optimize` | 67.9 µs |

```bash
cd in-cli && cargo bench
```

### in vs Rust: compile and binary comparison

| Metric | in (JIT) | in + cargo (debug) | Rust (cargo, release) |
|--------|----------|---------------------|----------------------|
| Compile fib(30) | **~30ms** | — | — |
| Self-host compile | **~12s** (parse + verify only) | **~120ms** | **~2s** (incremental) |
| On-disk binary | none (in-memory JIT) | ELF/Mach-O | ELF/Mach-O |
| Linker needed? | no | yes (via cargo) | yes (ld) |
| Languages | 40 | 1 (Rust via cargo backend) | 1 (Rust) |
| Output | mmap'd code page | native binary | native binary |

**Why in is fast**: no LLVM invocation, no linker process, no ELF generation for JIT.
JIT produces relocatable code bytes and dispatches in-process.
For native Rust binaries, in uses cargo/rustc as backend — but the frontend
analysis (parsing 3444 functions to Core IR) completes in ~50ms.

## MIR (Machine IR) Layer

Inspired by Zig's AIR → MIR → Emit pipeline, inauguration inserts a MIR stage
between Core IR and native codegen. MIR is offset-deferred assembly — 1:1 with
machine instructions but with symbolic operands and unresolved offsets. This
makes code relocatable: ideal for JIT mmap, where offsets are patched at the
last moment before execution.

Pipeline: **Source → Core IR → MIR → native_emit → JIT**.
