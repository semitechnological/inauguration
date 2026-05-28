# inauguration

`inauguration` is a language and general compiler project.

The native language is **`.in`**: a small, capability-aware systems and orchestration language designed around deterministic source, machine-readable structure, explicit effects, and compiler-managed execution graphs.

The compiler also acts as a general language ingestion pipeline. Multiple frontends lower into one shared **Core IR**, then move through common analysis and backend paths. The goal is one compiler architecture that can understand `.in`, `.icore`, C-family languages, Swift-shaped subsets, Rust, Go, V, OCaml, Java/Groovy body subsets, and other Tree-sitter-compatible source surfaces as the frontends mature.

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

The current `.in` surface is intentionally small and compiler-oriented.

Example:

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
- Java/Groovy bounded body lowering through Tree-sitter paths.

The maturity level varies by frontend. Some routes lower structured bodies; others currently expose declarations, signatures, parser facts, or `.icore` redirection hints. See [docs/architecture/parser-surface.md](docs/architecture/parser-surface.md), [docs/architecture/multi-frontend-ir.md](docs/architecture/multi-frontend-ir.md), and [docs/architecture/general-compiler.md](docs/architecture/general-compiler.md).

## Install CLI

**1. crates.io**

```bash
cargo install inauguration
```

Installs the `in` binary.

**2. Wax**

```bash
wax tap semitechnological/tap
wax install inauguration
```

**3. Install script from a clone**

```bash
./install.sh
```

**4. Build from source**

```bash
cargo build --release --manifest-path in-cli/Cargo.toml
```

The local binary is at `in-cli/target/release/in`.

## Core Commands

```bash
in build --parser in --path apps/in-sample/hello.in --module-id App
in build --parser in --path apps/in-sample/agent-native.in --module-id App
in build --parser icore --path apps/icore-sample/min.icore --module-id App
in agent --path apps/in-sample/agent-native.in --parser in
in graph --path apps/in-sample/agent-native.in --parser in --json
in package --path apps/in-sample/agent-native.in --json
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

## Development Notes

- Prefer owned compiler paths where possible.
- Use `.in` and `.icore` samples for language and Core IR changes.
- Keep frontend-specific behavior documented in `docs/architecture`.
- Keep package/capability behavior visible through `in package`, `in graph`, and `in agent`.
- Do not turn `.in` into a UI language.

