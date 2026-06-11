# `.in` Stdlib Runtime and Compiler Bootstrap Design

## Goal

Make `.in` capable of running the host-backed standard library operations needed to write compiler tooling, then use that runtime surface to bootstrap a small compiler written in `.in`.

## Scope

This spec has two ordered tracks:

1. **Rust-backed `.in` stdlib execution** for a compiler-useful subset.
2. **`.in` compiler bootstrap** that compiles a tiny expression language into an existing owned interchange format.

The first track must land before the second. The bootstrap compiler should use only stdlib operations proven by executable conformance tests.

## Non-Goals

- No network or HTTP runtime execution in this wave.
- No process spawning in this wave.
- No remote workers, GPU runtime, plugin loading, or package installation.
- No full replacement of Rust stdlib with `.in` implementations yet.
- No attempt to compile `in-cli` itself in the first bootstrap milestone.

## Rust-Backed Stdlib Runtime

The `.in` parser already synthesizes declarations for standard imports such as `std.fs`, `std.io`, `std.env`, and `std.path`. Today those declarations mostly provide graph shape and diagnostics. This wave makes a bounded subset executable in the bytecode VM by mapping known stdlib function calls to host intrinsics implemented in Rust.

### Initial Functions

`std.fs`:

- `read_file(path: String) -> String`
- `write_file(path: String, text: String) -> Bool`

`std.path`:

- `path_join(left: String, right: String) -> String`
- `path_dirname(path: String) -> String`
- `path_basename(path: String) -> String`
- `path_extname(path: String) -> String`

`std.env`:

- `env_get(name: String) -> String`
- `env_has(name: String) -> Bool`

`std.io`:

- `print(text: String) -> void`

### Capability Policy

Existing capability facts remain the public contract:

- `read_file` requires `fs.read`.
- `write_file` requires `fs.write`.
- `env_get` and `env_has` require `env.read`.
- `print` requires `process.stdout`.
- Path functions require no outside-world capability.

The runtime implementation should check capabilities when the existing compile or agent surface provides them. If runtime capability enforcement is not yet threaded to the VM, the first implementation should keep the existing diagnostics and add runtime enforcement in the next stdlib wave rather than silently inventing a second policy.

### Runtime Boundary

Rust backs these operations as host intrinsics. `.in` calls stay normal Core IR calls. Lowering and bytecode should recognize known stdlib function names and emit VM-callable operations. This keeps `.in` source and Core IR simple while allowing later `.in` implementations to replace individual intrinsics.

### Error Policy

The first executable stdlib returns simple values:

- Missing `env_get` returns an empty string.
- `env_has` reports whether the variable exists.
- `read_file` returns an empty string on read failure in the first slice, with diagnostics/error-result types deferred.
- `write_file` returns `false` on write failure and `true` on success.
- Path operations return UTF-8 lossy strings for host paths.

This policy is intentionally small so bootstrap compiler code can be written immediately. Rich `Result` types should be added after the bootstrap target exists.

## Compiler Bootstrap

Create `apps/in-compiler-bootstrap/` after executable stdlib basics land.

### First Target Language

The first compiler written in `.in` should compile a tiny expression language:

```text
let answer = 40 + 2
answer
```

The initial grammar should support:

- integer literals
- identifiers
- `let name = expr`
- binary `+`, `-`, `*`, `/`
- final expression result

### Output Format

The bootstrap compiler should emit `.icore` JSON first. `.icore` is already an owned interchange format and avoids making the bootstrap compiler emit textual SIL by hand. The validation pipeline is:

1. `.in` bootstrap compiler reads an expression-language source file.
2. It writes `.icore` JSON.
3. Current `in` compiles and executes the `.icore`.
4. Test asserts the expected result.

### Milestones

1. Stdlib VM intrinsics execute conformance fixtures.
2. `.in` tokenizer library can split a source string into token text.
3. `.in` parser can build enough structure for integer expressions and lets.
4. `.in` emitter writes valid `.icore`.
5. End-to-end bootstrap sample compiles a tiny source file and executes the generated `.icore`.

## Testing

Stdlib tests:

- Unit tests for bytecode/runtime intrinsic dispatch.
- `.in` conformance fixtures for file read/write, path operations, env reads, and print.
- Agent/package tests showing capabilities still report correctly.

Bootstrap tests:

- Tokenizer fixture.
- Parser fixture.
- `.icore` emitter fixture.
- End-to-end script under `scripts/` that runs the `.in` compiler and executes the generated `.icore`.

Required gates:

```bash
in update
in test
./scripts/check-in-lang-sample.sh
./scripts/check-icore-sample.sh
scripts/check-self-hosted-language-matrix.sh
git diff --check
```

## Open Risks

- The VM may need a cleaner host intrinsic boundary if stdlib operations are currently modeled only as ordinary empty extern functions.
- String and array ergonomics may become the real bootstrap bottleneck after file/path/env operations land.
- Returning empty strings for file/env failures is intentionally temporary; richer error values are needed before serious compiler code.

## First Implementation Plan

Start with executable stdlib functions in this order:

1. `std.path` intrinsics, because they are deterministic and low risk.
2. `std.env` read intrinsics.
3. `std.fs` read/write intrinsics.
4. `std.io.print`.
5. Bootstrap app scaffolding only after at least `std.path` and `std.fs` pass executable fixtures.
