# Native backend contract

`inauguration` owns two executable backend paths today:

1. **Bytecode VM subset** (all hosts): supported Core IR fronts lower to textual SIL, then to `.bca` bytecode assembly, then execute in the in-tree stack VM.
2. **Native exit-stub subset** (`aarch64-apple-darwin` only): scalar-return entry functions in the Core IR subset are const-evaluated through the bytecode pipeline and emitted as a tiny owned Mach-O executable that exits with the evaluated code. No `swiftc`, `clang`, or linker invocation occurs on this path.

Swift sources can still use `swiftc` for textual SIL or SwiftPM staging via `in build`, but that is a toolchain escape hatch, not the owned native backend. Use `in compile --target native` for owned native output; pass `--allow-external-toolchain` on `in build` only when external Swift/swiftc fallback is intentional.

## Stable status

### Bytecode backend

| Field | Value |
|-------|-------|
| `name` | `bytecode` |
| `implemented` | `true` (all hosts) |
| `stage` | `owned-runtime-subset` |
| `reason_code` | `bytecode-vm-subset` |
| `reason` | `inauguration owns this bytecode assembly format, SIL-to-bytecode lowering path, and stack VM runtime for the supported Core IR subset` |
| `input_stage` | `core-ir-to-textual-sil` |
| `artifact_kind` | `bytecode-assembly` |

### Native backend

| Host | `implemented` | `stage` | `reason_code` | `artifact_kind` |
|------|---------------|---------|---------------|-----------------|
| `aarch64-apple-darwin` | `true` | `owned-native-subset-aarch64` | `native-subset-aarch64` | `executable` |
| other | `false` | `contract-only` | `native-backend-not-implemented` | `none` |

On Apple Silicon macOS, `in compile --path apps/polyglot-sample/sample.in --target native --entry answer --out target/in/answer-sample` produces an owned executable that exits `42`. On Linux and other hosts, the same command reports `native-backend-not-implemented` and bytecode remains the primary owned executable path.

`in backend --path <file> --target bytecode --json` reports the owned bytecode backend and artifact facts for supported inputs. `in backend --target native --json` mirrors the host-specific native status above.

The target registry also carries checked-in In target equivalents for the Rust target triple matrix. These names are compiler target identities for planning, reports, manifests, and future lowering work. They do not imply object emission, linking, ABI lowering, or a native runtime until a target-specific backend is implemented and tested in this repository.

## Compile cache (Wave 6)

`in compile` hashes source path + content into `target/in/cache/<frontend_hash>/metadata.json`, storing the serialized owned compile report (including `frontend_hash`). Repeated compiles with the same frontend input reuse cached metadata when target, entry, and module id match.

## Scope

The native backend is the stage after source fronts, Core IR, textual SIL, and SIL analysis. It is responsible for turning a checked program into a runnable artifact without silently delegating code generation to a language toolchain.

| Area | First contract |
|------|----------------|
| Input | A checked Core IR subset with explicit functions, locals, calls, returns, and scalar values. |
| Output | `.bca` bytecode assembly on all hosts; Mach-O exit stubs on `aarch64-apple-darwin` for const-evaluable scalar entry functions. |
| Runtime | Only the runtime pieces present in this repository may be claimed. |
| Diagnostics | Unsupported constructs fail closed with `native-backend-not-implemented` or a narrower backend reason code. |
| Observability | Backend reports include input language, frontend level, IR stage, backend stage, artifact kind, timing, jobs, cache hit, and reason codes. |

## Non-goals for the first backend slice

- No claim of arbitrary Swift, C++, Rust, Go, V, JavaScript, Python, JVM, CLR, or Ruby native execution on every host.
- No silent `swiftc`, `clang`, `rustc`, `go`, `v`, or system linker fallback on a self-hosted native path.
- No broad ABI promise before the value model, call convention, symbols, object format, and runtime ownership are documented with tests.

## Integration points

- `docs/architecture/universal-compiler-roadmap.md`: keeps the native runtime spine and production-claim ladder honest.
- `docs/architecture/general-compiler.md`: defines the current source front to Core IR to textual SIL path.
- `in-cli/src/native_backend.rs`: owns backend status records for bytecode and native targets.
- `in-cli/src/native_emit/`: emits the aarch64 Mach-O exit stub.
- `in-cli/src/compile_cache.rs`: owns compile metadata cache under `target/in/cache/`.
- `in-cli/src/owned_compile.rs`: owned `in compile` pipeline and reports.
