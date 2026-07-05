# inlang (`.in`)

**inlang** is inauguration’s brace + line-oriented surface: one file format for compiler orchestration, agent reports, and bounded executable programs. It lowers to **Core IR** → **MIR** → **native JIT** (no LLVM, no bytecode VM).

**Crepuscularity** is the sibling UI stack (`.crepus`, GPUI, WASM sites). Inauguration owns compilation; crepuscularity owns rendering. The **docs-site** is a crepuscularity web target (`crepus web build` / `web serve`), same as crepuscularity’s own docs-site.

Workflow flags and CI entry points stay in the repo [README](../../README.md#core-commands). This page is grammar, imports, and IR shape.

## Quick start

```in
package demo;
module demo.main;

import std.io;

fn main() -> void {
  print("hello from inlang");
  return;
}
```

```bash
in build --path hello.in
in execute --path hello.in
in graph --path hello.in --json
```

## Ideology

- **Ultraminimal surface**: declarations first; bounded bodies in Core IR today; richer control flow grows in `in_lang_parse.rs` / `lower_core.rs`.
- **Agent-native**: `package`, `module`, `import`, `use`, `capability`, `extern`, and orchestration metadata feed `in agent` and `in graph` without hidden side effects.
- **Shared pipeline**: every front that emits `UnifiedModule` shares the same driver and JIT path ([multi-frontend-ir.md](multi-frontend-ir.md)).

## Top-level declarations (v0.2+)

| Form | Role |
|------|------|
| `package name;` / `module name;` | Dotted identity for reports (at most one each per file) |
| `import "./lib.in";` | Merge local `.in` declarations |
| `import std.io;` | Synthesize stdlib bindings (`print`, …) |
| `use pkg.key;` / `bind pkg.key as alias;` | Semantic package imports (manifest resolution) |
| `capability name;` | Declared outside-world capability facts |
| `enable ext;` | Orchestration extension fact (registry validation) |
| `struct` / `class` / `interface` / `component` | Types and contracts (see skill + conformance) |
| `fn name(params) -> Ret { … }` | Functions; **`fn main`** required for entry |
| `extern rust fn …;` | Foreign binding stub + capability `requires` |

### Stdlib imports

`std.io`, `std.fs`, `std.http`, `std.json`, `std.process`, `std.cli`, `std.env`, `std.path` add extern-style declarations. JIT-backed today includes **`print`**, **`read_file`**, **`write_file`**, **`process_run`**, **`env_get`**, **`env_set`**, **`env_has`**, and path helpers where wired in `native_stdlib` / `lower_stdlib`. `http_get`, full `json_parse`, and remote package install remain contract/diagnostic surfaces until runtime tests land.

### Orchestration (v0.4 contract)

| Command | Purpose |
|---------|---------|
| `in canonicalize --path <file>` | Deterministic `.in` formatting |
| `in graph --path <file> [--json]` | Parser decision, symbols, calls, package identity |
| `in package --path <dir\|manifest\|source>` | Manifest + dependency graph facts |
| `in agent` | Effects, capabilities, orchestration facts |

See [orchestration-compiler.md](orchestration-compiler.md).

## Types and bodies

- Types: `Int`, `String`, `Bool`, `void` / `Void`, named structs.
- Params: `name: Type`.
- Bodies: `let`, assign, `return`, `if` / `else`, `while`, calls, literals (bounded subset).

## Runtime boundaries

Treat roadmap bullets as **contracts** until `in test` and runtime code back them: GPU scheduling, remote `distributed fn`, semantic `use` install, and full FFI execution.

## `hybrid_sil` merge

Merged textual SIL keeps legacy last-`sil @` `function_id` while exposing per-function records in `SilArtifact::functions`. Emitters place **`@main` last** for single-function views. Details: [multi-frontend-ir.md](multi-frontend-ir.md).

## See also

- [docs-site.md](docs-site.md) — crepuscularity web site layout
- [general-compiler.md](general-compiler.md) — multi-language driver
- [native-backend.md](native-backend.md) — AArch64 / x86_64 JIT
- [parser-surface.md](parser-surface.md) — `ParserId` resolution