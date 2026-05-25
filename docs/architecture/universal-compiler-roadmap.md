# Universal compiler roadmap

`inauguration` will pursue one compiler architecture, not one giant parser rewrite. Every language enters through a front, lowers into shared Core IR or `.icore`, emits textual SIL through one lowering path, then reuses `hybrid_sil`, bytecode, metrics, and hot reload contracts where they apply.

The universal ambition is bounded by ownership. Inauguration owns orchestration, backend, and compiler infrastructure. Crepuscularity owns frontend UI, declarative view trees, rendering, and cross-platform visual abstraction. A UI framework may compile through inauguration, but UI runtime ownership is not part of this roadmap.

## North star

The target is a safe, small, fast compiler for V, Go, Rust, OCaml, TypeScript, JavaScript, Swift, Dart, Python, Java, Kotlin, C++, C#, C, Zig, Nim, Odin, Hare, Ruby, and adjacent languages. The promise is measured per language family:

| Goal | Meaning |
|------|---------|
| One compiler | One Core IR/SIL pipeline, shared diagnostics, shared capability reports, shared bytecode path where supported. |
| Safe | Unsupported semantics fail closed with a diagnostic or `.icore` redirect. No silent external compiler fallback on self-hosted paths. |
| Smaller | Native fronts and `.icore` emitters avoid bundling full language toolchains unless a runtime boundary explicitly requires it. |
| Faster | The default hot path avoids subprocess compilers for supported subsets and reports timing per stage. |
| Runtimes included | The repo owns the runtime boundary it claims: bytecode VM subset first, deterministic local orchestration simulation, Swift hot reload host for SwiftUI integration, and explicit non-bundled status for JVM, CLR, JS, Python, Ruby, libc, GPU execution, remote distributed execution, native machine-code execution, and language standard libraries until those runtimes have concrete in-tree implementations. |

## Compatibility ladder

| Level | Contract |
|-------|----------|
| 0 | Known language id and extension mapping only; source routes to a clear `.icore` redirect. |
| 1 | Source parses through a wired front and extracts declarations into `UnifiedModule`. |
| 2 | Bounded function bodies lower into Core IR and textual SIL. |
| 3 | The front typechecks enough language semantics to produce source-semantic diagnostics. |
| 4 | The front emits graph-aware facts, repair plans, and stable agent JSON. |
| 5 | The front has production build or hot reload semantics plus an owned runtime boundary. |

## Phased roadmap

| Phase | Deliverable | Exit criteria |
|-------|-------------|---------------|
| 0 | Truthful language matrix | `in languages` reports every target language, front, level, example, and runtime boundary as JSON and text. |
| 1 | Self-hosted Core IR backbone | `.in`, `.icore`, Rust, Go, V, Java/Groovy, and C-family bounded bodies lower through the same Core IR to SIL path with tests. |
| 2 | Web and scripting bodies | JavaScript, TypeScript, Python, and Ruby move from declaration extraction to bounded body lowering. |
| 3 | JVM and CLR semantics | Java, Kotlin, C#, and related fronts share class/method metadata, field access, constructor lowering, and runtime strategy docs. |
| 4 | Systems language depth | C, C++, Zig, Odin, Hare, Nim, Rust, Go, and V gain locals, calls, modules/packages, control flow, and ABI boundary docs. |
| 5 | Native runtime spine | Bytecode VM supports the common Core IR subset with deterministic execution, capability checks, and conformance examples. |
| 6 | Production claims | A language reaches level 5 only when examples compile and run without external compilers on the claimed path, quality gates are green, and benchmarks prove the speed claim for that scope. |

## v0.4 surface contract

Phase work should route through the strict v0.4 orchestration surfaces before claiming broader execution semantics:

| Surface | Roadmap meaning |
|---------|-----------------|
| Canonicalization | Source can be parsed, normalized, diagnosed, and compared deterministically for supported `.in` syntax. |
| Graph command | Compiler graph facts are visible as stable JSON: parser decision, declarations, functions, call edges, effects, capabilities, orchestration facts, entrypoint, reason codes, and timing. |
| Package manifest report | Package-level identity, targets, dependencies, capabilities, extensions, package graph nodes, target selection, capability policy, runtime boundaries, and parser-front decisions are reportable without installing or executing external systems. |
| Orchestration facts | Metadata such as `enable`, `@pure`, `@gpu`, `@parallel_safe`, `distributed fn`, and `parallel` is reported with local plan steps and local distributed job facts before remote runtime execution is claimed. |
| Backend report | `in backend` distinguishes the implemented bytecode VM subset from native machine-code status-only contracts. |

These are visibility and local-planning surfaces. They do not imply GPU kernels, remote workers, native binaries, package installation, or language runtime execution.

## Runtime policy

Runtime claims must be explicit. The compiler can lower a source language before it owns that language's runtime. A front may be useful at level 1 or 2 for graph facts, diagnostics, and tool-generated `.icore`, but it must not claim to include GPU execution, remote distributed worker execution, native machine-code execution, JVM, CLR, JavaScript, Python, Ruby, libc, C++ standard library, Dart VM, OCaml runtime, or SwiftUI runtime until the repo contains the runtime path and tests.

The first owned runtime is the existing bytecode VM subset fed by Core IR/SIL. The orchestration slice owns deterministic local planning and local distributed job simulation only. The SwiftUI preview host remains a Swift-specific runtime integration. Every new runtime must document supported values, calls, effects, ABI boundary, size budget, and benchmark method.

## Implementation order

1. Keep `in languages` and `docs/architecture/parser-surface.md` synchronized.
2. Promote one level at a time per language family.
3. Add examples before widening claims.
4. Run `in test` before push for the self-hosted compiler gates; run `in test --toolchain --external-parity` when implementation crates, protocol generation, Swift runtime integration, or reference compiler parity are in scope.
5. Benchmark only after correctness gates pass.

## Current slice

The current implementation has moved past the first matrix-only slice. OCaml now has a dedicated bounded front for top-level `let` functions and simple application expressions; Odin and Hare remain tracked level-0 `.icore` redirects until their own fronts land.

## See also

- [orchestration-compiler.md](orchestration-compiler.md) — v0.4 orchestration/status contract.
- [general-compiler.md](general-compiler.md) — source-front to Core IR to textual SIL architecture.
- [native-backend.md](native-backend.md) — native execution status contract.
