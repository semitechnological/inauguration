---
name: icore
description: "Use when reading, writing, or reviewing `.icore` JSON files. Covers the Core IR JSON format with icoreVersion, decls arrays, function/struct/class declarations, body statements, boundary ABI layouts, and symbol exports. Triggers: .icore file, icore format, Core IR JSON, icoreVersion, boundary ABI, layout, symbol export."
---

# `.icore` Skill

Guide for the inauguration `.icore` JSON format — Core IR serialized as JSON, consumed by `compiler::icore` front.

## Triggers

- Writing or reviewing `.icore` files
- `in build --parser icore` on `.icore` sources
- Core IR JSON structure questions
- Boundary ABI layouts and symbol exports
- `icoreVersion`, `decls`, `boundary` fields

## File Extension

`.icore` — resolved by `parser_registry.rs` via extension map or `#!in parser=icore` magic line.

## Format Reference

### Minimal valid `.icore`

```json
{
  "icoreVersion": 2,
  "decls": [
    {
      "kind": "function",
      "name": "main",
      "params": [],
      "return": "Void",
      "body": [
        { "kind": "return" }
      ]
    }
  ]
}
```

### Top-level keys

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `icoreVersion` | int | yes | Format version (2 = current, 3 = with boundary) |
| `decls` | array | yes | Declaration list, processed in order |
| `boundary` | object | no | ABI layout info (version 3 only) |

### Declaration kinds

#### Function

```json
{
  "kind": "function",
  "name": "helper",
  "params": [],
  "return": "Int",
  "body": [
    { "kind": "return", "value": { "kind": "int", "value": 7 } }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `kind` | string | `"function"` |
| `name` | string | Function symbol name |
| `params` | array | `[{ "name": "p", "type": "Type" }]` or `[]` |
| `return` | string | Return type: `"Int"`, `"String"`, `"Bool"`, `"Void"`, named struct |
| `body` | array | Statement list (optional — empty/absent = declaration only) |

#### Struct

```json
{
  "kind": "struct",
  "name": "Point",
  "fields": [
    { "name": "x", "type": "Int" },
    { "name": "y", "type": "Int" }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `kind` | string | `"struct"` |
| `name` | string | Struct type name |
| `fields` | array | `[{ "name": "f", "type": "T" }]` |

### Types

| Type string | Meaning |
|-------------|---------|
| `"Int"` | 64-bit integer |
| `"String"` | String |
| `"Bool"` | Boolean |
| `"Void"` | No return |
| `"Array"` | Dynamic array |
| `"Named"` | Named struct (use struct name directly) |

### Statement kinds

#### Return

```json
{ "kind": "return" }
{ "kind": "return", "value": { "kind": "int", "value": 42 } }
```

#### Let binding

```json
{ "kind": "let", "name": "x", "value": { "kind": "int", "value": 10 } }
```

#### Assignment

```json
{ "kind": "assign", "target": "x", "value": { "kind": "int", "value": 20 } }
```

#### If expression

```json
{
  "kind": "if",
  "cond": { "kind": "binary", "op": "==", "left": { "kind": "ident", "name": "x" }, "right": { "kind": "int", "value": 0 } },
  "then": [ { "kind": "return", "value": { "kind": "int", "value": 1 } } ],
  "else": [ { "kind": "return", "value": { "kind": "int", "value": 0 } } ]
}
```

#### While loop

```json
{
  "kind": "while",
  "cond": { "kind": "binary", "op": "<", "left": { "kind": "ident", "name": "i" }, "right": { "kind": "int", "value": 10 } },
  "body": [
    { "kind": "assign", "target": "i", "value": { "kind": "binary", "op": "+", "left": { "kind": "ident", "name": "i" }, "right": { "kind": "int", "value": 1 } } }
  ]
}
```

#### Expression statement

```json
{ "kind": "expr", "value": { "kind": "call", "callee": "print", "args": [ { "kind": "string", "value": "hello" } ] } }
```

### Expression kinds

#### Literals

```json
{ "kind": "int", "value": 42 }
{ "kind": "string", "value": "hello" }
{ "kind": "bool", "value": true }
{ "kind": "null" }
```

#### Identifier

```json
{ "kind": "ident", "name": "x" }
```

#### Binary operation

```json
{ "kind": "binary", "op": "+", "left": { "kind": "ident", "name": "a" }, "right": { "kind": "ident", "name": "b" } }
```

Ops: `+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`

#### Call

```json
{ "kind": "call", "callee": "function_name", "args": [ { "kind": "ident", "name": "x" } ] }
```

#### Struct init

```json
{
  "kind": "struct_init",
  "name": "Point",
  "fields": [
    { "name": "x", "value": { "kind": "int", "value": 1 } },
    { "name": "y", "value": { "kind": "int", "value": 2 } }
  ]
}
```

#### Field access

```json
{ "kind": "field", "object": { "kind": "ident", "name": "p" }, "field": "x" }
```

#### Array init

```json
{ "kind": "array_init", "elements": [ { "kind": "int", "value": 1 }, { "kind": "int", "value": 2 } ] }
```

#### Array access

```json
{ "kind": "array_access", "object": { "kind": "ident", "name": "a" }, "index": { "kind": "int", "value": 0 } }
```

#### Closure

```json
{
  "kind": "closure",
  "params": [ { "name": "x", "type": "Int" } ],
  "return": "Int",
  "body": [ { "kind": "return", "value": { "kind": "ident", "name": "x" } } ]
}
```

### Boundary (version 3)

```json
{
  "icoreVersion": 3,
  "decls": [ ... ],
  "boundary": {
    "abi_version": 1,
    "module": "sample.person",
    "layouts": [
      {
        "name": "Person",
        "kind": "struct",
        "repr": "c",
        "size": 24,
        "align": 8,
        "stride": 24,
        "fields": [
          { "name": "name", "offset": 0, "type": "InSliceU8", "transfer": "borrow" },
          { "name": "age", "offset": 16, "type": "u32", "transfer": "copy" }
        ]
      }
    ],
    "symbols": [
      {
        "name": "person_new",
        "signature_hash": "person_new_v1",
        "ownership": "returns-owned-handle",
        "calling_convention": "c"
      }
    ],
    "allocators": [
      { "id": 1, "kind": "host_arena", "free_with": "host" }
    ]
  }
}
```

#### Layout entry

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Type name |
| `kind` | string | `"struct"` |
| `repr` | string | ABI representation: `"c"` for C layout |
| `size` | int | Size in bytes |
| `align` | int | Alignment in bytes |
| `stride` | int | Stride (including padding) |
| `fields` | array | Field layout entries |

#### Field layout

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Field name |
| `offset` | int | Byte offset from struct start |
| `type` | string | ABI type: `"InSliceU8"`, `"u32"`, `"u64"`, etc. |
| `transfer` | string | Ownership transfer: `"borrow"`, `"copy"`, `"move"` |

#### Symbol entry

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Symbol name |
| `signature_hash` | string | Versioned signature hash |
| `ownership` | string | `"returns-owned-handle"`, `"borrows"`, etc. |
| `calling_convention` | string | `"c"`, `"rust"`, etc. |

#### Allocator entry

| Field | Type | Description |
|-------|------|-------------|
| `id` | int | Allocator ID |
| `kind` | string | `"host_arena"`, etc. |
| `free_with` | string | `"host"`, `"guest"`, etc. |

## Pipeline

`.icore` JSON → `compiler::icore` → `UnifiedModule` → `family_typecheck` → Core IR → SIL → bytecode → VM

## Key files

| File | Role |
|------|------|
| `in-cli/src/compiler/icore.rs` | `.icore` JSON parser |
| `in-cli/src/core_ir.rs` | Core IR data structures |
| `in-cli/src/lower_core.rs` | Core IR → textual SIL |

## See also

- `agents/skills/in-lang/SKILL.md` — `.in` source language
- `docs/architecture/core-ir-extensions.md` — classes, interfaces, closures, exceptions
- `docs/architecture/multi-frontend-ir.md` — `UnifiedModule`, parser resolution
