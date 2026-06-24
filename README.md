# inauguration

`inauguration` is a language and general compiler project. One **Core IR** → **native_emit/JIT** pipeline, 40 language frontends.

- **`.in` language**: capability-aware systems and orchestration language with canonical and human-friendly forms.
- **General compiler pipeline**: Tree-sitter polyglot frontends → Core IR → MIR → native_emit/JIT. No LLVM, no bytecode VM.
- **`in` CLI**: build, inspect, graph, package, test, run, and developer workflow.
- **Agent-readable facts**: JSON reports for parser decisions, imports, capabilities, effects, call graphs, diagnostics, and timing.
- **MIR layer**: Machine IR between Core IR and native emit for relocatable JIT code
- **Hot reload daemon**: in-process file watcher + patch planner via `in daemon`

Inauguration owns the language, compiler infrastructure, Core IR, backend contracts, orchestration facts, and runtime-boundary tooling. UI rendering belongs in sibling projects such as Crepuscularity.

## Install

```bash
cargo install inauguration
```

Or from source:

```bash
git clone https://github.com/semitechnological/inauguration.git
cd inauguration
./install.sh
```



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

### Compile time (JIT)

| Workload | Cold | Warm |
|----------|------|------|
| `fn main() -> Int { return 42 }` | **0.42ms** | **0.08ms** |
| fib(35) recursive (2 functions) | **37.8ms** | **0.12ms** |
| Self-host: `in-cli/src/lib.rs` (992 functions) | **666ms** | — |

### Compiler binary size

| Compiler | Size | Stripped | Dependencies |
|----------|------|----------|--------------|
| **in v0.6.5** (release, LTO) | **16MB** | 16MB | self-contained, no LLVM |
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

| Metric | in (JIT) | Rust (cargo, debug) | Rust (cargo, release) |
|--------|----------|---------------------|----------------------|
| Compile `return 42` | **0.42ms** | ~178ms | ~900ms |
| Compile fib(35) | **37ms** | — | — |
| On-disk binary | none (in-memory JIT) | 396KB ELF | ~4.7MB static |
| Linker needed? | no | yes (ld) | yes (ld) |
| Compiler self-size | 16MB | 0.4MB + LLVM deps | — |
| Languages | 40 | 1 (Rust) | 1 (Rust) |
| Output | mmap'd code page | ELF/Mach-O | ELF/Mach-O |

**Why in is fast**: no LLVM invocation, no linker process, no ELF generation.
JIT produces relocatable code bytes and dispatches in-process.
`cargo` must parse, typecheck, LLVM-IR-gen, optimize, link, and write ELF.

## MIR (Machine IR) Layer

Inspired by Zig's AIR → MIR → Emit pipeline, inauguration inserts a MIR stage
between Core IR and native codegen. MIR is offset-deferred assembly — 1:1 with
machine instructions but with symbolic operands and unresolved offsets. This
makes code relocatable: ideal for JIT mmap, where offsets are patched at the
last moment before execution.

Pipeline: **Source → Core IR → MIR → native_emit → JIT**.
