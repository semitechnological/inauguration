# `.in` Stdlib Runtime Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Execute compiler-useful `.in` stdlib functions through the existing bytecode VM, then scaffold the first `.in` compiler bootstrap app.

**Architecture:** Keep `.in` source and Core IR unchanged. Extend the existing SIL `function_ref` / `apply` lowering so known stdlib declarations become `Instruction::CallBuiltin`, and implement those builtins in `BytecodeVM` with Rust host operations. Add executable conformance fixtures before creating the bootstrap app.

**Tech Stack:** Rust 2024 in `in-cli`, Cargo for tests, existing `in` CLI and shell scripts for integration gates.

---

## File Structure

- `in-cli/src/sil_to_bytecode.rs`: recognize stdlib function names as bytecode builtins and keep return counts correct.
- `in-cli/src/vm.rs`: implement Rust-backed host intrinsics for `std.path`, `std.env`, `std.fs`, and `std.io`.
- `in-cli/src/in_lang_parse.rs`: adjust synthesized stdlib signatures only where they currently disagree with the spec.
- `conformance/runtime/stdlib-path.in`: executable `.in` fixture for path operations.
- `conformance/runtime/stdlib-env.in`: executable `.in` fixture for env reads.
- `conformance/runtime/stdlib-fs.in`: executable `.in` fixture for file read/write.
- `conformance/runtime/stdlib-io.in`: executable `.in` fixture for print as a void-returning intrinsic.
- `scripts/check-in-stdlib-runtime.sh`: focused integration gate for stdlib runtime fixtures.
- `apps/in-compiler-bootstrap/`: first bootstrap app, added only after stdlib fixtures pass.
- `docs/architecture/in-language.md`: update runtime-boundary wording once executable stdlib functions land.
- `todo.md`: record the completed stdlib/runtime slice and next compiler-bootstrap slice.

## Task 1: Path Builtins

**Files:**
- Modify: `in-cli/src/sil_to_bytecode.rs`
- Modify: `in-cli/src/vm.rs`
- Create: `conformance/runtime/stdlib-path.in`

- [ ] **Step 1: Write failing VM test for path builtins**

Add this test to the `#[cfg(test)]` module in `in-cli/src/vm.rs`:

```rust
#[test]
fn vm_std_path_builtins_execute() {
    let mut module = BytecodeModule::new("main".to_string());
    module.add_function(BytecodeFunction {
        name: "main".to_string(),
        local_count: 0,
        instructions: vec![
            Instruction::LoadString("/tmp".to_string()),
            Instruction::LoadString("demo.in".to_string()),
            Instruction::CallBuiltin("path_join".to_string(), 2),
            Instruction::CallBuiltin("path_basename".to_string(), 1),
            Instruction::Return,
        ],
    });
    let mut vm = BytecodeVM::new(module);
    let result = vm.run().expect("run path builtins");
    assert_eq!(result, Value::String("demo.in".to_string()));
}
```

- [ ] **Step 2: Run test and confirm RED**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_path_builtins_execute --locked
```

Expected: fail with `unknown builtin: path_join`.

- [ ] **Step 3: Implement path builtins**

In `BytecodeVM::call_builtin`, add match arms:

```rust
"path_join" => {
    let mut iter = args.into_iter();
    let left = iter.next().unwrap_or(Value::Nil).to_string_display();
    let right = iter.next().unwrap_or(Value::Nil).to_string_display();
    vec![Value::String(std::path::PathBuf::from(left).join(right).to_string_lossy().to_string())]
}
"path_dirname" => {
    let path = args.first().map_or(String::new(), Value::to_string_display);
    let parent = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    vec![Value::String(parent)]
}
"path_basename" => {
    let path = args.first().map_or(String::new(), Value::to_string_display);
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    vec![Value::String(name)]
}
"path_extname" => {
    let path = args.first().map_or(String::new(), Value::to_string_display);
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    vec![Value::String(ext)]
}
```

- [ ] **Step 4: Lower stdlib path calls as builtins**

In `in-cli/src/sil_to_bytecode.rs`, add these names to `is_builtin_function`:

```rust
| "path_join"
| "path_dirname"
| "path_basename"
| "path_extname"
```

- [ ] **Step 5: Add executable `.in` path fixture**

Create `conformance/runtime/stdlib-path.in`:

```in
import std.path;

fn main() -> String {
  let joined: String = path_join("/tmp", "compiler.in");
  return path_basename(joined);
}
```

- [ ] **Step 6: Verify path builtins**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_path_builtins_execute --locked
in build --path conformance/runtime/stdlib-path.in --target bytecode --run
```

Expected: Rust test passes and `.in` execution returns `String("compiler.in")`.

## Task 2: Env Builtins

**Files:**
- Modify: `in-cli/src/sil_to_bytecode.rs`
- Modify: `in-cli/src/vm.rs`
- Create: `conformance/runtime/stdlib-env.in`

- [ ] **Step 1: Write failing VM test for env builtins**

Add this test to `in-cli/src/vm.rs`:

```rust
#[test]
fn vm_std_env_builtins_execute() {
    std::env::set_var("IN_TEST_STDLIB_ENV", "present");
    let mut module = BytecodeModule::new("main".to_string());
    module.add_function(BytecodeFunction {
        name: "main".to_string(),
        local_count: 0,
        instructions: vec![
            Instruction::LoadString("IN_TEST_STDLIB_ENV".to_string()),
            Instruction::CallBuiltin("env_has".to_string(), 1),
            Instruction::Return,
        ],
    });
    let mut vm = BytecodeVM::new(module);
    let result = vm.run().expect("run env builtins");
    assert_eq!(result, Value::Bool(true));
}
```

- [ ] **Step 2: Run test and confirm RED**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_env_builtins_execute --locked
```

Expected: fail with `unknown builtin: env_has`.

- [ ] **Step 3: Implement env builtins**

Add these `BytecodeVM::call_builtin` match arms:

```rust
"env_get" => {
    let name = args.first().map_or(String::new(), Value::to_string_display);
    vec![Value::String(std::env::var(name).unwrap_or_default())]
}
"env_has" => {
    let name = args.first().map_or(String::new(), Value::to_string_display);
    vec![Value::Bool(std::env::var_os(name).is_some())]
}
```

- [ ] **Step 4: Lower env calls as builtins**

Add these names to `is_builtin_function`:

```rust
| "env_get"
| "env_has"
```

- [ ] **Step 5: Add executable `.in` env fixture**

Create `conformance/runtime/stdlib-env.in`:

```in
import std.env;
capability env.read;

fn main() -> Bool {
  return env_has("PATH");
}
```

- [ ] **Step 6: Verify env builtins**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_env_builtins_execute --locked
in build --path conformance/runtime/stdlib-env.in --target bytecode --run
```

Expected: Rust test passes and `.in` execution returns `Bool(true)`.

## Task 3: Filesystem Builtins

**Files:**
- Modify: `in-cli/src/in_lang_parse.rs`
- Modify: `in-cli/src/sil_to_bytecode.rs`
- Modify: `in-cli/src/vm.rs`
- Create: `conformance/runtime/stdlib-fs.in`

- [ ] **Step 1: Fix synthesized `write_file` return type**

In `in-cli/src/in_lang_parse.rs`, change `write_file` synthesized return type from `Typ::Void` to `Typ::Bool`:

```rust
ret: Typ::Bool,
```

Add or update a parser test so `import std.fs;` exposes `write_file(path: String, text: String) -> Bool`.

- [ ] **Step 2: Write failing VM test for fs builtins**

Add this test to `in-cli/src/vm.rs`:

```rust
#[test]
fn vm_std_fs_builtins_execute() {
    let path = std::env::temp_dir().join("in-stdlib-fs-test.txt");
    let path_text = path.to_string_lossy().to_string();
    let mut module = BytecodeModule::new("main".to_string());
    module.add_function(BytecodeFunction {
        name: "main".to_string(),
        local_count: 0,
        instructions: vec![
            Instruction::LoadString(path_text.clone()),
            Instruction::LoadString("hello compiler".to_string()),
            Instruction::CallBuiltin("write_file".to_string(), 2),
            Instruction::Pop,
            Instruction::LoadString(path_text),
            Instruction::CallBuiltin("read_file".to_string(), 1),
            Instruction::Return,
        ],
    });
    let mut vm = BytecodeVM::new(module);
    let result = vm.run().expect("run fs builtins");
    assert_eq!(result, Value::String("hello compiler".to_string()));
    let _ = std::fs::remove_file(path);
}
```

- [ ] **Step 3: Run test and confirm RED**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_fs_builtins_execute --locked
```

Expected: fail with `unknown builtin: write_file`.

- [ ] **Step 4: Implement fs builtins**

Add these `BytecodeVM::call_builtin` match arms:

```rust
"read_file" => {
    let path = args.first().map_or(String::new(), Value::to_string_display);
    vec![Value::String(std::fs::read_to_string(path).unwrap_or_default())]
}
"write_file" => {
    let mut iter = args.into_iter();
    let path = iter.next().unwrap_or(Value::Nil).to_string_display();
    let text = iter.next().unwrap_or(Value::Nil).to_string_display();
    vec![Value::Bool(std::fs::write(path, text).is_ok())]
}
```

- [ ] **Step 5: Lower fs calls as builtins**

Add these names to `is_builtin_function`:

```rust
| "read_file"
| "write_file"
```

- [ ] **Step 6: Add executable `.in` fs fixture**

Create `conformance/runtime/stdlib-fs.in`:

```in
import std.fs;
capability fs.read;
capability fs.write;

fn main() -> String {
  let ok: Bool = write_file("/tmp/in-stdlib-runtime-fixture.txt", "hello compiler");
  return read_file("/tmp/in-stdlib-runtime-fixture.txt");
}
```

- [ ] **Step 7: Verify fs builtins**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_fs_builtins_execute --locked
in build --path conformance/runtime/stdlib-fs.in --target bytecode --run
```

Expected: Rust test passes and `.in` execution returns `String("hello compiler")`.

## Task 4: Focused Stdlib Runtime Gate

**Files:**
- Create: `scripts/check-in-stdlib-runtime.sh`
- Modify: `docs/architecture/in-language.md`
- Modify: `todo.md`

- [ ] **Step 1: Create focused script**

Create `scripts/check-in-stdlib-runtime.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

in build --path conformance/runtime/stdlib-path.in --target bytecode --run
in build --path conformance/runtime/stdlib-env.in --target bytecode --run
in build --path conformance/runtime/stdlib-fs.in --target bytecode --run
```

Make it executable:

```bash
chmod +x scripts/check-in-stdlib-runtime.sh
```

- [ ] **Step 2: Verify focused script**

Run:

```bash
scripts/check-in-stdlib-runtime.sh
```

Expected: all three fixtures execute successfully.

- [ ] **Step 3: Update docs**

In `docs/architecture/in-language.md`, update the standard-import section to say `std.fs`, `std.env`, `std.path`, and `std.io.print` now have bytecode VM execution for the listed bounded functions, while `std.http`, `std.process`, package installation, and plugin execution remain contract-only.

- [ ] **Step 4: Update roadmap**

In `todo.md`, add a bullet under the `.in` agent-native surface noting that Rust-backed bytecode execution exists for the bounded stdlib subset and that the next slice is compiler bootstrap scaffolding.

## Task 5: Bootstrap App Skeleton

**Files:**
- Create: `apps/in-compiler-bootstrap/README.md`
- Create: `apps/in-compiler-bootstrap/sample.expr`
- Create: `apps/in-compiler-bootstrap/compiler.in`
- Create: `scripts/check-in-compiler-bootstrap.sh`

- [ ] **Step 1: Create bootstrap README**

Create `apps/in-compiler-bootstrap/README.md`:

```markdown
# in compiler bootstrap

This app is the first bootstrap target for a compiler written in `.in`.

Milestone 0 depends on executable `std.fs` and `std.path`. It reads a tiny expression-language source file and will emit `.icore` in the next implementation slice.
```

- [ ] **Step 2: Create sample expression source**

Create `apps/in-compiler-bootstrap/sample.expr`:

```text
let answer = 40 + 2
answer
```

- [ ] **Step 3: Create `.in` bootstrap reader skeleton**

Create `apps/in-compiler-bootstrap/compiler.in`:

```in
import std.fs;
import std.path;
capability fs.read;

fn main() -> String {
  let path: String = path_join("apps/in-compiler-bootstrap", "sample.expr");
  return read_file(path);
}
```

- [ ] **Step 4: Create bootstrap smoke script**

Create `scripts/check-in-compiler-bootstrap.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

in build --path apps/in-compiler-bootstrap/compiler.in --target bytecode --run
```

Make it executable:

```bash
chmod +x scripts/check-in-compiler-bootstrap.sh
```

- [ ] **Step 5: Verify bootstrap skeleton**

Run:

```bash
scripts/check-in-compiler-bootstrap.sh
```

Expected: execution returns the contents of `sample.expr`.

## Task 6: Final Verification

**Files:**
- Review all changed files.

- [ ] **Step 1: Reinstall local `in`**

Run:

```bash
in update
```

Expected: local binary installs from this checkout.

- [ ] **Step 2: Run focused gates**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml vm_std_path_builtins_execute --locked
cargo test --manifest-path in-cli/Cargo.toml vm_std_env_builtins_execute --locked
cargo test --manifest-path in-cli/Cargo.toml vm_std_fs_builtins_execute --locked
scripts/check-in-stdlib-runtime.sh
scripts/check-in-compiler-bootstrap.sh
```

Expected: all pass.

- [ ] **Step 3: Run repo gates**

Run:

```bash
in test
./scripts/check-in-lang-sample.sh
./scripts/check-icore-sample.sh
scripts/check-self-hosted-language-matrix.sh
git diff --check
```

Expected: all pass. If `apps/package-ecosystem-sample/inauguration.lock` changes only because an npm `latest` resolved to a newer version during verification, inspect the diff and restore that unrelated generated churn before staging.

- [ ] **Step 4: Commit and push**

Run:

```bash
git status --short
git add in-cli/src/sil_to_bytecode.rs in-cli/src/vm.rs in-cli/src/in_lang_parse.rs conformance/runtime/stdlib-path.in conformance/runtime/stdlib-env.in conformance/runtime/stdlib-fs.in scripts/check-in-stdlib-runtime.sh apps/in-compiler-bootstrap/README.md apps/in-compiler-bootstrap/sample.expr apps/in-compiler-bootstrap/compiler.in scripts/check-in-compiler-bootstrap.sh docs/architecture/in-language.md todo.md docs/superpowers/plans/2026-06-11-in-stdlib-runtime-bootstrap.md
git commit -m "Add in stdlib runtime bootstrap"
git push
```

Expected: commit and push succeed on the existing branch.

## Self-Review

- Spec coverage: The plan implements Rust-backed stdlib execution for path, env, fs, and an existing print builtin, then creates the bootstrap app skeleton once file/path execution works.
- Placeholder scan: No step relies on unstated behavior; every code-changing task names files, code shape, commands, and expected output.
- Scope check: Full tokenizer/parser/emitter bootstrap is intentionally deferred until stdlib execution exists. This plan creates the runtime base and a smoke app that proves `.in` can read compiler input.
