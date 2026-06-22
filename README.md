# inauguration

`inauguration` is a language and general compiler project.

The native language is **`.in`**: a small, capability-aware systems and orchestration language designed around deterministic source, machine-readable structure, explicit effects, and compiler-managed execution graphs. The language is meant to have two writing modes over the same semantics: an explicit canonical form for compilers, agents, code review, and generated source; and a human-friendly form that keeps everyday code easy to read while canonicalizing into the strict form.

The compiler also acts as a general language ingestion pipeline. Multiple frontends lower into one shared **Core IR**, then move through common analysis and backend paths. The goal is one compiler architecture that can understand `.in`, `.icore`, C-family languages, Swift-shaped subsets, Rust, Go, V, OCaml, Java/Groovy, JavaScript/TypeScript, Python/Ruby, Zig, and other Tree-sitter-compatible source surfaces as the frontends mature.

## What It Is

- **`.in` language**: top-level imports, capabilities, extern bindings, structs, functions, bounded bodies, annotations, distributed-function facts, and parallel-region facts.
- **General compiler pipeline**: parser selection, frontend lowering, Core IR, textual SIL lowering, bytecode/native backend work, graph reports, and package/capability metadata.
- **`in` CLI**: build, inspect, graph, package, test, run, and developer workflow commands.
- **Agent-readable compiler facts**: JSON reports for parser decisions, imports, capabilities, effects, call graphs, orchestration facts, diagnostics, repair plans, and timing.
- **Swift and hot reload tooling**: Swift-shaped subset work, optional Swift toolchain paths, protocol models, and a SwiftUI preview/hotreload runtime.
- **Plugin hooks**: project accelerators and compiler workflow extensions under `plugins/registry`.

Inauguration is not a frontend UI framework. Frontend rendering and declarative UI belong in sibling projects such as Crepuscularity. Inauguration owns the language, compiler infrastructure, Core IR, backend contracts, orchestration facts, package/capability reporting, and runtime-boundary tooling.

## Repository Layout

- `in-cli`: the `in` CLI, `.in` parser, Core IR paths, owned compile reporting, graph/package commands, bytecode/native backend work, hotreload daemon sources, and protocol regeneration.
- `compiler/rust-driver`: orchestration pipeline, stage model, SIL analysis, and batch compiler path.
- `runtime/hotreload-daemon`: thin daemon wrapper and integration tests.
- `runtime/swift-preview-host`: Swift package that receives and applies reload envelopes.
- `apps/in-sample`: minimal `.in` modules for language and Core IR checks.
- `apps/icore-sample`: `.icore` JSON Core IR modules.
- `apps/native-subset-sample`: small Swift-shaped subset sample for the in-tree Swift subset compiler.
- `plugins/registry`: installable project accelerators.
- `docs/architecture`: language, compiler, parser, backend, interop, and roadmap documents.
- `docs-site`: static documentation site.
- `scripts`: validation, generation, install, and workflow scripts.

## `.in` Language

The current `.in` surface is intentionally small and compiler-oriented, but the language direction is flexible syntax over stable semantics.

There are two intended ways to write the same program:

- **Explicit form**: every capability, external boundary, type, return value, and orchestration fact is visible. This is the form generated tools should prefer.
- **Human form**: short, readable source that can omit ceremony where the compiler can infer or canonicalize it. This is the form people should enjoy writing.

The strict parser and `in canonicalize` path are the source of truth today. Human-friendly spellings should lower into the same Core IR and canonical output rather than becoming a second language.

Explicit form:

```in
import std.io;

capability process.stdout;

extern rust fn host_log(text: String) -> void requires process.stdout;

struct Message {
  String text
}

fn main() -> void {
  print("hello from .in")
  host_log("compiler-visible effect")
  return
}
```

The same idea in the human-facing direction:

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

That second spelling is the readability target: fewer separators, indentation where it helps, and lightweight calls. The compiler should still be able to produce the explicit form for review, tooling, reproducible builds, and machine edits.

Current language features include:

- `import path;`
- `capability name;`
- `extern <language> fn name(...) -> Type requires capability.name;`
- `struct Name { Type field }`
- `fn name(params) -> Type { ... }`
- bounded `let`, assignment, `return`, calls, literals, `if`, `while`, and expression statements
- annotations such as `@pure`, `@gpu`, and `@parallel_safe`
- `distributed fn` as a compiler-visible orchestration fact
- `parallel { ... }` as a deterministic local planning fact

Flexible syntax goals include:

- `import std.io;` and semicolon-free `import std.io`
- `use database.postgres;` as a semantic package import resolved through `inauguration.package`
- `capability process.stdout;` and `needs process.stdout`
- `fn main() -> void { ... }` and `main: ...`
- `print("hello")` and `print "hello"`
- brace-delimited blocks and indentation-delimited blocks
- explicit `return value` and expression-oriented final values where safe
- generated canonical output for every accepted human spelling

More examples:

Capability-first service boundary:

```in
import std.fs;
import std.json;

capability fs.read;
capability fs.write;

extern host fn read_config(path: String) -> String requires fs.read;
extern host fn write_report(path: String, text: String) -> void requires fs.write;

struct Report {
  String title
  String body
}

fn main() -> void {
  let raw: String = read_config("space.config")
  let report: Report = Report { title: "Space", body: raw }
  write_report("space.report", report.body)
  return
}
```

Orchestration facts:

```in
enable distributed-workers;

capability gpu.compute;

struct Frame {
  Int id
}

@gpu
distributed fn render_frame(frame: Frame) -> void {
  return
}

@parallel_safe
fn warm_cache() -> void {
  return
}

parallel {
  warm_cache();
}

fn main() -> void {
  warm_cache()
  return
}
```

Human-facing equivalent direction:

```in
enable distributed-workers

needs gpu.compute

Frame:
  id: Int

@gpu
distributed render_frame(frame: Frame):
  done

@parallel_safe
warm_cache:
  done

parallel:
  warm_cache

main:
  warm_cache
```

See [docs/architecture/in-language.md](docs/architecture/in-language.md) for the exact grammar and status.

## General Compiler Pipeline

The compiler frontends lower source into `UnifiedModule` Core IR. Core IR then feeds shared lowering and analysis paths.

Primary source surfaces today:

- `.in`: native language frontend.
- `.icore`: JSON interchange for Core IR.
- Swift-shaped subset: hermetic in-tree subset path.
- Swift via toolchain integration where needed.
- Rust, Go, V, and OCaml dedicated bounded frontends.
- C / C++ / Objective-C++ and other Tree-sitter parser routes.
- Shared bounded scalar body lowering through Tree-sitter AST conventions where wired.

The maturity level varies by frontend. Some routes lower structured bodies; others currently expose declarations, signatures, parser facts, or `.icore` redirection hints. See [docs/architecture/parser-surface.md](docs/architecture/parser-surface.md), [docs/architecture/multi-frontend-ir.md](docs/architecture/multi-frontend-ir.md), and [docs/architecture/general-compiler.md](docs/architecture/general-compiler.md).

## Install CLI

**1. crates.io**

```bash
cargo install inauguration
```

Installs the `in` binary after a public crate release. During prerelease work, install from a clone.

**2. Wax**

```bash
wax tap semitechnological/tap
wax install inauguration
```

Use after the tap release is available.

**3. Install script from a clone**

```bash
./install.sh
```

## Language Support

40 languages. Capabilities indicate what the compiler can do with each frontend:

- **parse** — Tree-sitter AST extraction
- **lower** — Core IR body emission
- **typecheck** — Family-aware semantic type checking
- **boundary** — Cross-language Boundary IR + ABI layout
- **bytecode** — Bytecode VM compilation

| Language | Parser | parse | lower | typecheck | boundary | bytecode |
|----------|--------|-------|-------|-----------|----------|----------|
| in | in | ✓ | ✓ | ✓ | ✓ | ✓ |
| icore | icore | ✓ | ✓ | ✓ | ✓ | ✓ |
| Swift | swift | ✓ | ✓ | ✓ | — | — |
| Rust | rust | ✓ | ✓ | ✓ | ✓ | — |
| Go | go | ✓ | ✓ | ✓ | — | — |
| V | v | ✓ | ✓ | ✓ | ✓ | ✓ |
| C | c | ✓ | ✓ | ✓ | — | — |
| C++ | c | ✓ | ✓ | ✓ | — | — |
| Objective-C | c | ✓ | ✓ | — | — | — |
| Objective-C++ | c | ✓ | ✓ | ✓ | — | — |
| Java | java | ✓ | ✓ | ✓ | — | — |
| Groovy | groovy | ✓ | ✓ | ✓ | — | — |
| JavaScript | javascript | ✓ | ✓ | ✓ | ✓ | ✓ |
| TypeScript | typescript | ✓ | ✓ | ✓ | ✓ | ✓ |
| Kotlin | kotlin | ✓ | ✓ | ✓ | — | — |
| Scala | scala | ✓ | ✓ | ✓ | — | — |
| C# | c | ✓ | ✓ | ✓ | — | — |
| F# | f | ✓ | ✓ | — | — | — |
| VB.NET | vbnet | ✓ | ✓ | ✓ | ✓ | — |
| Python | python | ✓ | ✓ | ✓ | — | — |
| Ruby | ruby | ✓ | ✓ | ✓ | — | — |
| PHP | php | ✓ | ✓ | ✓ | — | — |
| Perl | perl | ✓ | ✓ | — | — | — |
| Zig | zig | ✓ | ✓ | ✓ | ✓ | — |
| Dart | dart | ✓ | ✓ | ✓ | — | — |
| Lua | lua | ✓ | ✓ | ✓ | — | — |
| Clojure | clojure | ✓ | ✓ | ✓ | ✓ | — |
| Elixir | elixir | ✓ | ✓ | — | — | — |
| Erlang | erlang | ✓ | ✓ | — | — | — |
| Haskell | haskell | ✓ | ✓ | — | — | — |
| Nim | nim | ✓ | ✓ | ✓ | ✓ | — |
| OCaml | ocaml | ✓ | ✓ | ✓ | — | — |
| Julia | julia | ✓ | ✓ | — | — | — |
| R | r | ✓ | ✓ | — | — | — |
| D | d | ✓ | ✓ | ✓ | ✓ | — |
| Crystal | crystal | ✓ | ✓ | ✓ | ✓ | — |
| Odin | odin | ✓ | ✓ | ✓ | ✓ | ✓ |
| Hare | hare | ✓ | ✓ | ✓ | ✓ | — |
| HolyC | holyc | ✓ | ✓ | — | — | — |


## Core Commands

```bash
in build --parser in --path apps/in-sample/hello.in --module-id App
in build --parser in --path apps/in-sample/agent-native.in --module-id App
in build --parser icore --path apps/icore-sample/min.icore --module-id App
in agent --path apps/in-sample/agent-native.in --parser in
in graph --path apps/in-sample/agent-native.in --parser in --json
in package --path apps/package-sample/main.in --json
in languages
in languages --json
in explain INAGENT020 --json
in fix --plan --json --path apps/in-sample/hello.in --parser in
in backend --path apps/in-sample/agent-native.in --target bytecode --json
in backend --target native --json
in dev
in run
in test
in doctor
```

## Plugin Commands

```bash
in plugin list
in plugin install aurorality
in plugin install crepuscularity
in plugin run aurorality --target ../aurorality
```

## Validation

Required full check:

```bash
in test
```

Useful focused checks:

```bash
cd compiler/rust-driver && cargo test --all
cd in-cli && cargo test
cd runtime/hotreload-daemon && cargo test
./scripts/check-protocol-models.sh
./scripts/check-native-subset-sample.sh
./scripts/check-in-lang-sample.sh
./scripts/check-icore-sample.sh
./scripts/check-polyglot-sample.sh
```

Run Swift preview-host checks when touching the Swift runtime:

```bash
cd runtime/swift-preview-host && swift build -Xswiftc -warnings-as-errors && swift test
```

## Protocol Models

Rust `protocol-gen` is the canonical checked-in code generator from `shared/protocol/events.schema.json`:

```bash
cargo run --manifest-path in-cli/Cargo.toml --bin protocol-gen
./scripts/check-protocol-models.sh
```

V remains available only for optional parity tooling such as `shared/protocol/generate_models.v`; the Rust generator is the CI source of truth.

# License

MPL-2.0
