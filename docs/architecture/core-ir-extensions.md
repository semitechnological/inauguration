# Core IR, SIL, Bytecode & Runtime Extensions — Design & Implementation

**Status**: Wave 1 Phase A — Core IR data structures implemented; parser syntax and runtime still gated.

## Current State (post-Wave 1 Phase A)

| Layer | What exists |
|-------|-------------|
| Core IR | `Typ` (Int/String/Bool/Void/Array/Named), `Expr` (literals, binary, struct init, field, array, call, **Closure**), `Stmt` (let, assign, return, if, loop, match, expr, **Throw**, **Try**), `Decl` (Struct, Function, **Class**, **Interface**), `UnifiedModule` |
| Core IR new types | `Visibility` (Pub/Private/Internal), `CatchArm`, `MethodSig`, `Import` |
| Bytecode | 19 stack-based opcodes, `Value` (Int/Bool/String/Struct field data/Array/Nil), text roundtrip (.bca) |
| VM | Stack frames, locals, globals, 6 builtins (print, print_int, print_string, to_int, to_string, len) |
| SIL lowering | Core IR → textual SSA SIL with `function_ref`/`apply`, constant folding |
| SIL→bytecode | Textual SIL → bytecode with peephole opts |
| Struct methods | Desugared at parse time: `p.sum()` → `Point_sum(p)` via name mangling. No virtual dispatch. |

### Implementation Status Detail

| Feature | Data structure | Parser (`.in`) | Typecheck | Lowering | Fronts emitting |
|---------|---------------|----------------|-----------|----------|-----------------|
| `Decl::Class` | ✅ `core_ir.rs:202` | ❌ | ✅ match arm | ✅ match arm | Rust, Go, V fronts |
| `Decl::Interface` | ✅ `core_ir.rs:204` | ❌ | ✅ match arm | ❌ | — |
| `Expr::Closure` | ✅ `core_ir.rs:47` | ✅ (fixed) | ✅ | ✅ match arm | — |
| `Stmt::Throw` | ✅ `core_ir.rs:78` | ❌ | ✅ | ✅ | — |
| `Stmt::Try`/`CatchArm` | ✅ `core_ir.rs:79` | ❌ | ✅ | ✅ match arm | — |
| `Visibility` | ✅ `core_ir.rs:107` | — | — | — | On Class/Interface only |
| `is_async` on Function | ❌ (design only) | — | — | — | — |
| `Visibility` on Function/Struct | ❌ (design only) | — | — | — | — |
| `Closure::captures` | ❌ (design only) | — | — | — | — |

## Design Principles

1. **Minimum viable complexity** — add only what's demonstrably needed across multiple language fronts
2. **Desugar where possible** — keep bytecode/VM simple; push complexity to Core IR → SIL lowering
3. **Follow existing patterns** — same enum style, same naming, same `Result<_, String>` error convention
4. **Test end-to-end** — every new Core IR node must roundtrip through parse → IR → SIL → bytecode → VM

---

## Wave 1: Classes, Interfaces, Closures, Exceptions, Async

### 1. Core IR Extensions

#### 1.1 Classes (`Decl::Class`)

```rust
// core_ir.rs additions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Pub,
    Private,
    Internal,
}

Decl::Class {
    name: String,
    fields: Vec<(String, Typ)>,
    methods: Vec<Decl>,            // Decl::Function variants
    visibility: Visibility,
    extends: Option<String>,       // parent class name (single inheritance)
    implements: Vec<String>,       // interface names
}

Decl::Interface {
    name: String,
    methods: Vec<MethodSig>,       // method signatures only (no body)
    visibility: Visibility,
}

pub struct MethodSig {
    pub name: String,
    pub params: Vec<(String, Typ)>,
    pub ret: Typ,
}
```

**Design rationale**: Classes collect fields + methods in one decl. Interfaces declare contracts. Single inheritance (`extends`), multiple interface conformance (`implements`). Constructors are implicit (no separate syntax) — a `new` method or `StructInit`-style initializer.

#### 1.2 Closures/Lambdas (`Expr::Closure`)

```rust
Expr::Closure {
    params: Vec<(String, Typ)>,
    ret: Typ,
    body: Vec<Stmt>,
    captures: Vec<String>,         // captured variable names
}
```

**Design rationale**: Closures capture by value (copy). Lowering desugars each closure to a hidden `Decl::Function` + a struct holding captures. The closure expression becomes `StructInit { capture_values... }` + a call to the hidden function.

#### 1.3 Exceptions (`Stmt::Throw`, `Stmt::Try`)

```rust
Stmt::Throw(Expr),

Stmt::Try {
    body: Vec<Stmt>,
    catches: Vec<CatchArm>,
}

pub struct CatchArm {
    pub pattern: String,           // match pattern or "_" for catch-all
    pub body: Vec<Stmt>,
}
```

**Design rationale**: Throw unwinds to nearest enclosing try/catch. Catches match on error value (string or int). No typed exception hierarchy yet — values only. If no catch matches, VM halts with error.

#### 1.4 Async annotation on Functions

```rust
Decl::Function {
    // ... existing fields ...
    is_async: bool,                // NEW
    visibility: Visibility,        // NEW
}
```

**Design rationale**: Minimal — just a flag. Full async lowering (state machines, task scheduling) is Wave 3. For Wave 1, `is_async` is metadata consumed by fronts (e.g., Swift `async` → Core IR flag).

#### 1.5 Visibility on all decls

`Visibility` enum added to `Decl::Function`, `Decl::Struct`, `Decl::Class`, `Decl::Interface`. Default is `Visibility::Internal`.

### 2. SIL Lowering Extensions

#### 2.1 Class lowering strategy

Classes **desugar to structs + mangled functions** at the Core IR → SIL boundary:

```
// Input: Decl::Class { name: "Dog", fields: [("age", Int)], methods: [bark()] }
// Output:
//   Decl::Struct { name: "Dog", fields: [("age", Int)] }
//   Decl::Function { name: "Dog_bark", params: [("self", Named("Dog"))], ... }

// Method call p.bark() desugars to: Dog_bark(p)
```

**Why desugar**: No new bytecode opcodes or VM features needed. All existing infrastructure works. The `.in` front already does this at parse time; we move it to the lowering layer so ALL fronts benefit.

#### 2.2 Interface lowering

Interfaces produce no runtime artifacts. They're checked at typecheck time and used for documentation/cross-front validation.

#### 2.3 Closure lowering

Closures desugar to: (1) a hidden top-level function, (2) a struct for captures, (3) `StructInit` filling captures, (4) a `Call` to the hidden function with captures struct as first arg.

#### 2.4 Exception lowering

Throw/catch lower to early-return patterns in SIL using the existing `cond_br`/`br`/`label` infrastructure. The VM gets a new builtin `throw_error` that unwinds the call stack. Try/catch sets an error handler marker.

### 3. Bytecode Extensions

#### 3.1 New opcodes (optional for Wave 1 if desugaring covers all needs)

```
Instruction::NewObj(String, Vec<String>)  // class name, field names
Instruction::MethodCall(String, usize)     // method name, arg count
Instruction::SuperCall(String, String, usize) // class name, method name, arg count
```

**Decision**: Since classes desugar at Core IR → SIL level, these opcodes are NOT needed for Wave 1. The existing `StructInit` + `CallFunction` opcodes suffice. Add these in Wave 2 when we want a heap object model with virtual dispatch.

#### 3.2 Value type extension

```
Value::Object { class_name: String, fields: Vec<(String, Value)> }
```

Same shape as `Value::Struct` but with a class tag for method dispatch. Added in Wave 2.

### 4. VM Extensions

#### 4.1 Heap object model (Wave 2)

For Wave 1, no VM changes needed — desugaring handles method dispatch at compile time.

Wave 2 would add:
- `heap: Vec<Value>` for object allocation
- `method_tables: HashMap<String, HashMap<String, usize>>` for vtable dispatch
- `Instruction::NewObj` → allocates on heap, returns index
- `Instruction::MethodCall` → looks up method in class's vtable, dispatches

#### 4.2 New builtins (Wave 1)

```
"throw_error"  // pops error value, unwinds to nearest catch or halts
```

### 5. Runtime Services (Future Waves)

| Service | Wave | Description |
|---------|------|-------------|
| Heap/GC | 2 | Mark-and-sweep or ref counting for objects |
| Scheduler | 3 | Async task executor with cooperative scheduling |
| Stdin/FS | 4 | `read_file`, `write_file`, stdin access |
| HTTP | 5 | `http_get`, `http_post` |
| Package resolution | 4 | Resolve module paths to files |

---

## Implementation Plan: Wave 1

### Phase A: Core IR extensions ✅ COMPLETE (partial: data structures only)

1. ✅ Add `Visibility` enum — `core_ir.rs:107`
2. ❌ Add `is_async: bool` to `Decl::Function` — NOT YET
3. ❌ Add `visibility: Visibility` to all `Decl` variants — only Class/Interface done
4. ✅ Add `Decl::Class` with fields, methods, visibility, extends, implements — `core_ir.rs:202`
5. ✅ Add `Decl::Interface` with `MethodSig` struct — `core_ir.rs:204`
6. ✅ Add `Expr::Closure` — `core_ir.rs:47` (no `captures` field yet)
7. ✅ Add `Stmt::Throw` and `Stmt::Try` with `CatchArm` — `core_ir.rs:78-85`
8. ✅ Update `core_typecheck.rs` for all new nodes — match arms present
9. ✅ Update `lower_core.rs` to handle new nodes — match arms present
10. ❌ Update `in_lang_parse.rs` to emit `Decl::Class` — parser yet to add class/try/catch syntax

### Phase B: Exception runtime support ❌ NOT STARTED

1. ❌ Add `"throw_error"` builtin to VM
2. ❌ Add try/catch stack markers to VM call frames

### Phase C: Conformance fixtures ✅ STUBBED

1. ✅ Conformance directory created at `conformance/` with 9 subdirs and 22 `.in` fixtures
2. ❌ Class/error fixtures are design stubs (`.in` parser doesn't yet support syntax)
3. ❌ parse → IR → bytecode → VM execution tests in `bytecode_compiler.rs`

### Phase D: Verification ✅

1. ✅ Full test suite passes: 436 tests, 0 failures
2. ✅ Self-hosted language matrix gate passes: 14 PASS, 2 SKIP, 0 FAIL

---

## Compatibility Notes

- All existing `Decl::Function` and `Decl::Struct` variants keep their current shape
- `UnifiedModule` unchanged
- All existing tests pass without modification
- Backward-compatible: structs with methods in `.in` files continue to work (desugaring moves from parse time to lowering, semantics identical)
