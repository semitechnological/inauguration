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
| java     | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 3 metadata; bounded sample bytecode executes after family type normalization |
| kotlin   | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering; bounded sample bytecode executes |
| cs       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering; bounded sample bytecode executes |
| fsharp   | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 1 declaration front; bounded sample bytecode executes |
| swift    | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: subset or swiftc textual SIL path |
| rust     | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| go       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| v        | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: dedicated bounded body lowering |
| js       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 5: bounded entrypoint + Boundary IR + bytecode VM |
| ts       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 5: bounded entrypoint + Boundary IR + bytecode VM |
| python   | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| ruby     | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 2: tree-sitter scalar body lowering |
| vb       | ✅    | ✅          | ✅    | ✅                 | 0                    | Level 3 boundary front; bounded sample bytecode executes |

**Verdict**: 28/28 runnable matrix entries pass the self-hosted gate. 2 languages (objc, objcpp) skipped — no sample files exist.

## Core IR feature conformance by frontend

Feature support at the Core IR level (data structures exist), but frontends vary in emission:

| Feature | `.in` parser | Rust front | Go front | V front | tree_front | Swift subset |
|---------|-------------|------------|-----------|----------|------------|-------------|
| `Decl::Struct` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Decl::Function` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Decl::Class` | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| `Decl::Interface` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Expr::Closure` | ✅ (expression) | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Stmt::Throw` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Stmt::Try` | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `Import` | ✅ (surface) | ❌ | ❌ | ❌ | ❌ | ❌ |

## Conformance fixture coverage

| Directory | Fixtures | Status |
|-----------|----------|--------|
| `conformance/types/` | 4 | ✅ Runs — primitive, struct, array, named types |
| `conformance/control-flow/` | 4 | ✅ Runs — if/else, loop, match, nested branches |
| `conformance/functions/` | 4 | ✅ Runs — basic fn, params, expressions, void |
| `conformance/classes/` | 9 | ✅ Runs — class syntax, methods, inheritance/interface shapes, Java class fixtures |
| `conformance/errors/` | 6 | ✅ Runs — throw, try-catch, function throw, caught and uncaught paths |
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

1. **Async execution semantics** — async fixtures parse, but scheduling/execution semantics remain design-stage
2. **`is_async` field** — declared in design, not yet on `Decl::Function`
3. **`Visibility` on Function/Struct** — only Class and Interface have visibility
4. **Production class/interface runtime semantics** — `.in` parser/conformance coverage exists, but full dispatch/runtime policy remains bounded
5. **Advanced exception semantics** — local throw/try-catch paths lower and execute, but typed exceptions and cross-function unwinding remain future work
