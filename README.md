# inauguration

`inauguration` is a language and general compiler project. One **Core IR** and SIL pipeline, many frontends.

- **`.in` language**: capability-aware systems and orchestration language with canonical and human-friendly forms.
- **General compiler pipeline**: Tree-sitter polyglot frontends, Core IR, textual SIL, bytecode/native backend, graph reports.
- **`in` CLI**: build, inspect, graph, package, test, run, and developer workflow.
- **Agent-readable facts**: JSON reports for parser decisions, imports, capabilities, effects, call graphs, diagnostics, and timing.
- **Swift and hot reload**: subset compiler, protocol models, SwiftUI preview/hotreload runtime.

Inauguration owns the language, compiler infrastructure, Core IR, backend contracts, orchestration facts, and runtime-boundary tooling. Frontend rendering belongs in sibling projects such as Crepuscularity.

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
| `in-cli` | CLI, `.in` parser, Core IR, compile reporting, graph/package commands, bytecode/native backend, hotreload daemon, protocol gen |
| `compiler/rust-driver` | orchestration pipeline, stage model, SIL analysis, batch path |
| `runtime/hotreload-daemon` | thin daemon wrapper + tests |
| `runtime/swift-preview-host` | Swift package for reload envelopes |
| `apps/in-sample` | `.in` language and Core IR samples |
| `apps/icore-sample` | `.icore` JSON Core IR modules |
| `apps/native-subset-sample` | Swift-shaped subset sample |
| `plugins/registry` | project accelerators |
| `docs/architecture` | language, compiler, interop, roadmap docs |
| `scripts` | validation, generation, install, workflow |

## Language Support

39 Tree-sitter parsers + native `.in`/`.icore` frontends, all sharing one Core IR.

| Language | parse | lower | typecheck | boundary | bytecode |
|----------|:-----:|:-----:|:---------:|:--------:|:--------:|
| in | ✓ | ✓ | ✓ | ✓ | ✓ |
| icore | ✓ | ✓ | ✓ | ✓ | ✓ |
| Swift | ✓ | ✓ | ✓ | — | — |
| Rust | ✓ | ✓ | ✓ | ✓ | — |
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

*\* Objective-C: typecheck partial*

## Core Commands

```bash
in build --parser in --path apps/in-sample/hello.in --module-id App
in build --parser icore --path apps/icore-sample/min.icore --module-id App
in agent --path apps/in-sample/agent-native.in --parser in
in graph --path apps/in-sample/agent-native.in --parser in --json
in package --path apps/package-sample/main.in --json
in languages --json
in explain INAGENT020 --json
in fix --plan --json --path apps/in-sample/hello.in --parser in
in backend --path apps/in-sample/agent-native.in --target bytecode --json
in run
in test
in doctor
```

## `.in` Language

Two forms over the same semantics: **explicit** (generated, reviewable) and **human** (readable, canonicalizes to explicit).

Explicit:

```in
import std.io;
capability process.stdout;
extern rust fn host_log(text: String) -> void requires process.stdout;

struct Message { String text }

fn main() -> void {
  print("hello from .in")
  host_log("compiler-visible effect")
}
```

Human:

```in
import std.io
needs process.stdout
host_log(text: String) uses process.stdout
Message:
  text: String
main:
  print "hello from .in"
  host_log "compiler-visible effect"
```

Features: `import`, `capability`/`needs`, `extern`, `struct`, `fn`, `let`, `if`, `while`, `return`, annotations (`@pure`, `@gpu`, `@parallel_safe`), `distributed fn`, `parallel { }`.

See [docs/architecture/in-language.md](docs/architecture/in-language.md) for grammar and status.

## Performance

macOS M5 Pro (arm64), self-hosted compiler.

### Binary size

| Compiler | Size |
|----------|------|
| **in** (release) | 73 MB |
| go build (add.go) | 1.7 MB |

### Compile time: `add(40, 2)`

| Compiler | Time | Output |
|----------|------|--------|
| **in** JIT | 0.5ms | native in-memory |
| **in** bytecode | 9ms | bytecode VM |
| go build | 290ms | native |
| rustc (debug) | 400ms | native |
| swiftc | 1200ms | native |

### Execution: `fib(35)`

| Runtime | Time | vs Go |
|---------|------|-------|
| **in** JIT | 0.4ms | 325× faster |
| Go native | 130ms | baseline |
| **in** bytecode | 16,500ms | 130× slower |

### Self-host: 992 Rust functions

| | Bytecode | JIT |
|---|----------|-----|
| Parsed | 992 | 992 |
| Lowered | 184 | 370 |
| Cold | 616ms | 713ms |
| Warm | 22ms | 755ms |

## Validation

```bash
in test
```

Focused:

```bash
cd compiler/rust-driver && cargo test --all
cd in-cli && cargo test
cd runtime/hotreload-daemon && cargo test
./scripts/check-protocol-models.sh
```

Swift (when toolchain available):

```bash
cd runtime/swift-preview-host && swift build -Xswiftc -warnings-as-errors && swift test
```

Set `IN_TEST_SKIP_SWIFT=1` to skip Swift steps.

## License

MPL-2.0
