---
name: in-lang
description: "Use when reading, writing, or reviewing `.in` source files. Covers package/module declarations, imports, structs, classes, functions, control flow, memory ops, I/O, capabilities, orchestration annotations, and the component/interface system. Triggers: .in file, in-lang, inauguration language, in build, in parse, in graph, in agent, package, module, import, capability, component, interface."
---

# `.in` Language Skill

Guide for the inauguration `.in` source language — brace + line-oriented, lowers to Core IR → SIL → bytecode.

## Triggers

- Writing or reviewing `.in` files
- `in build`, `in parse`, `in graph`, `in agent` on `.in` sources
- Package/module/import questions
- Struct, class, function, control flow in `.in`
- Capability, component, interface declarations
- Memory ops (`load64`, `store64`), I/O (`outb`, `inb`), CPU (`hlt`, `sti`, `cli`)

## File Extension

`.in` — resolved by `parser_registry.rs` via extension map or `#!in parser=in` magic line.

## Grammar Reference

### Top-level declarations

```in
package space.kernel;          // dotted identity (optional, one per file)
module space.kernel.main;      // dotted identity (optional, one per file)

import "./lib.in";             // local .in import (merges declarations)
import std.io;                 // stdlib import (synthesizes extern decls)
use database.postgres;         // semantic package import (needs inauguration.package)
bind database.postgres as pg;  // explicit binding (suppresses INPKG002)
capability serial: DebugConsole(write);  // outside-world capability fact
enable distributed-workers;    // orchestration extension fact
```

### Stdlib imports

`std.io`, `std.fs`, `std.http`, `std.json`, `std.process`, `std.cli`, `std.env`, `std.path` — each synthesizes bounded extern-style Core IR declarations. Check `in agent` for capability requirements.

### Annotations (metadata, not execution)

```in
@pure
fn add(a: Int, b: Int) -> Int { return a + b; }

@gpu
fn render() -> void { return; }

@parallel_safe
fn hash(data: String) -> Int { return 0; }
```

### Constants and variables

```in
const MAX_SIZE = 64
const PCI_CONFIG_ADDR = 0xCF8

var heap_next: Int = 0
var line: Int = 0
```

### Structs

```in
struct KernelState {
  Int root_table_id
  Int realm_id
  Bool cpu_ready
}

struct Point {
  Int x
  Int y
}
```

Fields: `Type name` separated by semicolons or line breaks. Types must be built-ins or struct names declared above.

### Classes

```in
class Dog {
  name: String

  fn bark() -> String {
    return "woof";
  }
}

fn main() -> String {
  let d = Dog { name: "Rex" };
  return d.bark();
}
```

Classes desugar to structs + mangled functions at Core IR → SIL: `Dog_bark(d)`.

### Interfaces

```in
interface BootEntry {
  fn start(mb_info: Int) -> Int
}

interface DebugConsole {
  fn write(msg: String) -> void
}
```

Interfaces declare contracts only — no runtime artifacts.

### Components

```in
component KernelRoot {
  target "x86_64-unknown-none"
  deterministic true
  checkpoint none

  export boot: BootEntry

  capability serial: DebugConsole(write)
  capability memory: PhysicalMemory(discover, map)
}
```

Components declare authority, export interfaces, and declare capability requirements. The SCI loader rule enforces that requested capabilities are a subset of realm grants.

### Functions

```in
fn answer() -> Int {
  return 42;
}

fn main() -> void {
  let a: Int = answer();
  return;
}

fn pci_read32(bus: Int, dev: Int, func: Int, off: Int) -> Int {
  outl(PCI_CONFIG_ADDR, pci_addr(bus, dev, func, off))
  return inl(PCI_CONFIG_DATA)
}
```

- `fn name(params) -> Ret` — `fn` only, no `func`/`function` in v0
- Parameters: `param: Type` comma-separated
- Types: `Int`, `String`, `Bool`, `void`/`Void`, named structs/classes
- `fn main` required (same as Swift subset)

### Extern declarations

```in
extern rust fn malloc(size: Int) -> Int;
extern c fn free(ptr: Int) -> void requires capability.memory;
```

Declares external bindings without embedding a foreign parser. Optional `requires capability.name` attaches capability contracts.

### Distributed functions

```in
distributed fn train_model(data: String) -> Int {
  return 0;
}
```

Parsed as orchestration fact, exposed as local simulated worker job. No remote execution yet.

### Parallel regions

```in
parallel {
  process_chunk(data, 0)
  process_chunk(data, 1)
  process_chunk(data, 2)
}
```

Counted as orchestration region, lowered to deterministic local plan steps. No threaded execution yet.

### Control flow

```in
if flag == true {
  return "ready";
} else if flag == false {
  return "wait";
} else {
  return "unknown";
}

while n > 0 {
  n = n - 1
}

for i in 0..8 {
  sum = sum + a[i]
}
```

### Expressions

```in
let a = 42
let b = a + 10
let c = (a << 16) | b
let d = some_function(arg1, arg2)
let e = obj.method()
```

Binary: `+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`

### Memory operations (bare-metal / kernel)

```in
let val = load64(addr)
let val = load32(addr)
let val = load8(addr)
store64(addr + 0, value)
store32(addr + 4, value)
store8(addr + 0, 48)
```

### I/O ports

```in
outb(port, value)
outl(port, value)
let val = inb(port)
let val = inl(port)
```

### CPU operations

```in
hlt()
sti()
cli()
pause()
invlpg(virt)
lidt(idtr)
let cr2 = read_cr2()
```

### Function pointer invocation

```in
invoke(entry)
invoke1(fn_ptr, arg1)
invoke2(fn_ptr, arg1, arg2)
```

### Array access

```in
let a = alloc(64)
a[0] = 42
let v = a[i]
```

Alloc returns Int (pointer), arrays are raw pointer + offset.

## Comments

```in
// line comment
```

## Conformance test annotations

```in
// @expect parse: ok
// @expect bytecode: executes
// @expect result: String("woof")
// @expect has-function: main
// @expect has-call-edge: answer
```

## Pipeline

`.in` source → `in_lang_parse.rs` → `UnifiedModule` → `family_typecheck` → Core IR → `lower_core.rs` → textual SIL → bytecode → VM

## Key files

| File | Role |
|------|------|
| `in-cli/src/in_lang_parse.rs` | Parser implementation |
| `in-cli/src/lower_core.rs` | Core IR → textual SIL |
| `in-cli/src/core_ir.rs` | Core IR data structures |
| `in-cli/src/parser_registry.rs` | Extension/magic-line resolution |

## See also

- `docs/architecture/in-language.md` — full grammar spec
- `docs/architecture/parser-surface.md` — extension → front mapping
- `docs/architecture/core-ir-extensions.md` — classes, interfaces, closures, exceptions
- `agents/skills/icore/SKILL.md` — `.icore` JSON format
