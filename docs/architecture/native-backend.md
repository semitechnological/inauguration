# Native backend contract

`inauguration` does not yet own a native machine-code backend. The current self-hosted compiler path stops at Core IR, textual SIL, `hybrid_sil` analysis, and the bytecode VM subset. Swift sources can still use `swiftc` for textual SIL or SwiftPM staging, but that is a toolchain escape hatch, not an in-tree native backend.

## Stable status

| Field | Value |
|-------|-------|
| `implemented` | `false` |
| `stage` | `contract-only` |
| `reason_code` | `native-backend-not-implemented` |
| `reason` | `inauguration currently has no in-tree object-file emitter, linker driver, ABI lowering, or owned machine runtime for native code generation` |
| `current_executable_boundary` | `Core IR and textual SIL may feed the bytecode VM subset where supported; full Swift builds still rely on external Swift tooling when requested` |

Any CLI, library API, agent report, or test fixture that needs to answer "can inauguration emit a native binary by itself?" should use the same reason code until an in-tree backend lands.

## Scope

The native backend is the stage after source fronts, Core IR, textual SIL, and SIL analysis. It is responsible for turning a checked program into a runnable artifact without silently delegating code generation to a language toolchain.

The first supported backend should be deliberately small:

| Area | First contract |
|------|----------------|
| Input | A checked Core IR or lowered SIL subset with explicit functions, locals, calls, returns, and scalar values. |
| Output | A deterministic artifact format selected by the backend slice: object file, executable, or owned bytecode handoff promoted to a native target. |
| Runtime | Only the runtime pieces present in this repository may be claimed. Missing libc, Swift, JVM, CLR, JavaScript, Python, Ruby, or other language runtimes must remain explicit boundaries. |
| Diagnostics | Unsupported constructs fail closed with `native-backend-not-implemented` or a narrower backend reason code. |
| Observability | Backend reports include input language, frontend level, IR stage, backend stage, artifact kind, timing, and reason codes. |

## Non-goals for the first backend slice

- No claim of arbitrary Swift, C++, Rust, Go, V, JavaScript, Python, JVM, CLR, or Ruby native execution.
- No silent `swiftc`, `clang`, `rustc`, `go`, `v`, or system linker fallback on a self-hosted native path.
- No broad ABI promise before the value model, call convention, symbols, object format, and runtime ownership are documented with tests.
- No new backend dependency until its license, telemetry behavior, artifact size, target support, and failure modes are documented.

## Roadmap

| Phase | Deliverable | Exit criteria |
|-------|-------------|---------------|
| 0 | Status contract | Documentation and any public status API report `implemented=false` with `native-backend-not-implemented`. |
| 1 | Backend trait and report | A small in-tree interface accepts a typed backend request and returns either a rejected status or an artifact descriptor. |
| 2 | Minimal executable subset | One owned target can run a tiny Core IR fixture with scalar return values and no external runtime claims. |
| 3 | Calls and symbols | Internal function calls, deterministic symbol names, and duplicate-symbol diagnostics are covered by fixtures. |
| 4 | Data and memory | Struct fields, strings, arrays, heap or stack allocation policy, and lifetime boundaries are documented and tested. |
| 5 | Runtime boundary | Capability-checked effects, host calls, and any bundled runtime support are explicit in `in languages --json`. |
| 6 | Production claim | A level-5 language family can build and run through the native backend with quality gates and benchmarks checked in. |

## Backend request contract

A future library module should expose a small status surface before it exposes artifact generation. The first shape can be implemented without dependencies:

| Field | Meaning |
|-------|---------|
| `implemented` | Whether an in-tree native backend can produce the requested artifact. |
| `reason_code` | Stable machine-readable reason for rejection or limitation. |
| `reason` | Human-readable explanation for CLI and agent output. |
| `input_stage` | `core-ir`, `textual-sil`, `bytecode`, or a narrower stage once the implementation chooses one. |
| `artifact_kind` | Requested output such as `object`, `executable`, or `none`. |

Until Phase 1 lands, all native backend status reports should be equivalent to:

```json
{
  "implemented": false,
  "reason_code": "native-backend-not-implemented",
  "reason": "inauguration currently has no in-tree object-file emitter, linker driver, ABI lowering, or owned machine runtime for native code generation",
  "input_stage": "core-ir-or-textual-sil",
  "artifact_kind": "none"
}
```

## Integration points

- `docs/architecture/universal-compiler-roadmap.md`: keeps the native runtime spine and production-claim ladder honest.
- `docs/architecture/general-compiler.md`: defines the current source front to Core IR to textual SIL path.
- `docs/architecture/parser-surface.md`: reports per-language frontend levels and runtime boundaries.
- `in-cli/src/language_support.rs`: should remain the machine-readable language support surface until a backend status API is added.
- `in-cli/src/compiler/driver.rs` and `in-cli/src/lower_core.rs`: define the current Core IR to textual SIL handoff.

## Follow-up implementation tasks

1. Thread the status into CLI or agent JSON only after the public output shape is agreed.
2. Add a fixture-driven backend request type before any object-file or executable generation work starts.
3. Add object-file or executable generation only after the value model, call convention, symbol format, and runtime boundary have tests.
