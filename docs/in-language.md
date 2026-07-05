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

## Syntax by example

Small, copy-pasteable shapes the parser and Core IR accept today. Params are always **`name: Type`**.

### Package, module, imports

```in
package demo.app;
module demo.app.main;

import "./helpers.in";
import std.io;
import std.process;

fn main() -> void {
  print("ok");
  return;
}
```

Semantic package wiring (needs `inauguration.package` nearby):

```in
use my.lib;
bind my.lib as lib;
```

### Types, constants, variables

```in
const MAX = 128;
var counter: Int = 0;

fn bump() -> Int {
  counter = counter + 1;
  return counter;
}
```

Built-ins: `Int`, `String`, `Bool`, `void` / `Void`, plus struct/class names you declare.

### Struct and class

```in
struct Point {
  Int x;
  Int y;
}

class Greeter {
  name: String;

  fn hello() -> String {
    return "hi " + name;
  }
}

fn main() -> String {
  let p = Point { x: 1, y: 2 };
  let g = Greeter { name: "Ada" };
  return g.hello();
}
```

### Interface and component (contracts)

```in
interface Writer {
  fn write(msg: String) -> void;
}

component AppShell {
  target "aarch64-apple-darwin"
  deterministic true

  export main: BootEntry
  capability serial: DebugConsole(write)
}
```

Interfaces are surface contracts; components declare exports, targets, and capability needs for `in agent` / SCI-style checks.

### Functions, control flow, calls

```in
fn fib(n: Int) -> Int {
  if n <= 1 {
    return n;
  } else {
    return fib(n - 1) + fib(n - 2);
  }
}

fn sum_to(limit: Int) -> Int {
  let acc: Int = 0;
  let i: Int = 0;
  while i < limit {
    acc = acc + i;
    i = i + 1;
  }
  return acc;
}

fn main() -> Int {
  return fib(10) + sum_to(5);
}
```

`for i in 0..8 { … }` is recognized where the front supports bounded ranges.

### Extern and capabilities

```in
capability memory;

extern rust fn alloc(size: Int) -> Int;
extern c fn free(ptr: Int) -> void requires memory;

fn main() -> void {
  return;
}
```

Missing `capability` lines for `requires` show up as `in agent` warnings (`AGENT_MISSING_CAPABILITY`).

### Orchestration metadata

```in
enable distributed-workers;

@pure
fn hash(data: String) -> Int {
  return 0;
}

distributed fn train(data: String) -> Int {
  return 0;
}

parallel {
  step(0);
  step(1);
}

fn step(id: Int) -> void {
  return;
}
```

`distributed`, `parallel`, and `enable` are **facts** for graphs and agents; remote/threaded execution is not guaranteed yet.

### Stdlib-style I/O (JIT builtins)

```in
import std.io;
import std.fs;
import std.process;

fn main() -> void {
  print("hello");
  write_file("/tmp/out.txt", "data");
  process_run("./scripts/build-docs-site.sh");
  return;
}
```

### Kernel / bare-metal subset (when lowering allows)

```in
const PORT: Int = 0x3F8;

fn uart_put(c: Int) -> void {
  outb(PORT, c);
  return;
}

fn read_word(addr: Int) -> Int {
  return load64(addr);
}

fn halt_loop() -> void {
  while true {
    hlt();
  }
}
```

`load*` / `store*`, `inb` / `outb`, `hlt`, `sti`, `cli`, and friends target freestanding emit paths; availability depends on target and type support in `native_emit`.

### Expressions and operators

```in
fn ops(a: Int, b: Int) -> Bool {
  let sum = a + b;
  let masked = (a << 4) | (b & 0xFF);
  return sum == 42 && masked != 0;
}
```

Arithmetic, bitwise, and comparisons chain as in the table in [parser-surface.md](parser-surface.md); method calls desugar to mangled functions on classes.

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