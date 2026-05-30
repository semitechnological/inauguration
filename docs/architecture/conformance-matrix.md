# Conformance Matrix

Languages against Core IR feature gates and pipeline stages. Each cell is the
highest level of conformance observed through the own pipeline on the polyglot
sample.

## Self-hosted language gate results

Run: `scripts/check-self-hosted-language-matrix.sh`

| Language | Build | Graph (JSON) | Agent | Backend (bytecode) | External invocations | Notes |
|----------|-------|-------------|-------|--------------------|-----------------------|-------|
| in       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 3: bounded subset + source diagnostics |
| icore    | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: versioned Core IR JSON |
| c        | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| cpp      | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| objc     | SKIP  | —           | —     | —                  | —                    | No sample file; parser_id objc routes at level 1 |
| objcpp   | SKIP  | —           | —     | —                  | —                    | No sample file; parser_id objcpp routes at level 2 |
| java     | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter bounded body lowering |
| kotlin   | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| cs       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| swift    | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: subset or swiftc textual SIL path |
| rust     | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| go       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| v        | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| js       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter bounded body lowering |
| ts       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter bounded body lowering |
| python   | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |

**Verdict**: 14/14 mandatory languages pass the self-hosted gate. 2 languages (objc, objcpp) skipped — no sample files exist.

## Core IR feature conformance by frontend

Feature support at the Core IR level (data structures exist), but frontends vary in emission:

| Feature | `.in` parser | Rust front | Go front | V front | tree_front | Swift subset |
|---------|-------------|------------|-----------|----------|------------|-------------|
| `Decl::Struct` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Decl::Function` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Decl::Class` | ❌ parser TODO | ✅ | ✅ | ✅ | ❌ | ❌ |
| `Decl::Interface` | ❌ parser TODO | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Expr::Closure` | ✅ (expression) | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Stmt::Throw` | ❌ parser TODO | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Stmt::Try` | ❌ parser TODO | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Import` | ✅ (surface) | ❌ | ❌ | ❌ | ❌ | ❌ |

## Conformance fixture coverage

| Directory | Fixtures | Status |
|-----------|----------|--------|
| `conformance/types/` | 4 | ✅ Runs — primitive, struct, array, named types |
| `conformance/control-flow/` | 4 | ✅ Runs — if/else, loop, match, nested branches |
| `conformance/functions/` | 4 | ✅ Runs — basic fn, params, expressions, void |
| `conformance/classes/` | 2 | ⚠️ DESIGN — struct+fn pattern, class syntax not yet in parser |
| `conformance/errors/` | 3 | ⚠️ DESIGN — try/catch/throw stubs, safe fallback patterns |
| `conformance/modules/` | 3 | ✅ Runs — package, capabilities, local import |
| `conformance/async/` | 1 | 📋 STUB — Wave 3 design |
| `conformance/runtime/` | 1 | 📋 STUB — Waves 4-5 design |
| `conformance/packages/` | 2 | ✅ Runs — manifest, no-manifest |

## Pipeline stage maturity

| Stage | Maturity | Description |
|-------|----------|-------------|
| Parse (`.in`) | ✅ Stable | v0.2 grammar, structs, functions, control flow |
| Parse (icore JSON) | ✅ Stable | v1/v2 versioned JSON schema |
| Parse (tree_front) | ✅ Growing | 15+ languages with scalar body lowering |
| Parse (Swift subset) | ✅ Stable | func/struct/control flow parsing and checking |
| Core IR | ✅ Growing | Wave 1 data structures present; partial parser coverage |
| SIL lowering | ✅ Stable | Core IR → textual SSA SIL |
| SIL→bytecode | ✅ Stable | Textual SIL → bytecode with peephole opts |
| VM | ✅ Stable | Stack frames, 6 builtins |
| Native AArch64 | ✅ Stable | Owned Mach-O emitter for scalar subset |
| Async | ❌ Design | Wave 3 |
| GC/runtime | ❌ Design | Waves 2-5 |

## Remaining gaps (Wave 1)

1. **`.in` parser class syntax** — `class Dog { ... }` not yet parsed; `Decl::Class` only emitted by Rust/Go/V fronts
2. **`.in` parser try/catch/throw syntax** — not yet parsed; `Stmt::Try`/`Throw` are match-arm stubs
3. **`is_async` field** — declared in design, not yet on `Decl::Function`
4. **`Visibility` on Function/Struct** — only Class and Interface have visibility
5. **`Closure::captures` field** — missing from `Expr::Closure`
6. **`throw_error` VM builtin** — not yet implemented
7. **Exception lowering** — `lower_core.rs` has match arms but no desugaring logic
