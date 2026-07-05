# Inauguration Parallel Roadmap Wave Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the current open roadmap items into a bounded, parallel implementation wave that advances `.in`, native artifact identity, Swift subset depth, conformance/docs, and local gates without overlapping edits.

**Architecture:** Split work by ownership boundary. Each agent owns one surface and must avoid editing another agent's files except for the final coordinator task. Shared integration happens only after all focused tests pass.

**Tech Stack:** Rust 2024 in `in-cli`, Cargo via `cargo`, repository CLI via `in`, SwiftPM only through `in test` or existing scripts, no JS package manager required.

---

## File Structure

- `in-cli/src/native_emit/lower.rs`: native artifact and ABI manifest identity propagation.
- `in-cli/src/boundary_ir.rs`: ABI manifest schema for package/module identity if the existing schema has no identity field.
- `in-cli/src/boundary_emit.rs`: JSON emission for ABI identity fields.
- `in-cli/src/swift_subset.rs`: Swift subset parser/checker additions.
- `in-cli/src/native_swift_sil.rs`: subset-to-SIL behavior and focused snapshot tests.
- `in-cli/src/in_lang_parse.rs`: `.in` parser/typechecker gaps for runtime/error semantics only; class/interface parsing already exists and must not be duplicated.
- `in-cli/src/lower_core.rs`: Core IR lowering for `.in` exception/runtime completion where missing.
- `in-cli/src/sil_to_bytecode.rs`: bytecode lowering for exception/runtime completion where missing.
- `in-cli/src/bytecode.rs`: VM/runtime support if `throw_error` or equivalent builtins are missing there.
- `docs/architecture/conformance-matrix.md`: update stale claims after implementation.
- `docs/architecture/parser-surface.md`: update language levels and next steps from live behavior.
- `docs/architecture/native-swift-master-plan.md`: update Swift subset phase status.
- `todo.md`: mark only completed slices and leave future work explicit.

## Parallel Execution Rules

- Agent A may edit only native artifact identity files and tests.
- Agent B may edit only Swift subset files and tests.
- Agent C may edit only `.in` runtime/error lowering files and tests.
- Agent D may edit only docs, scripts, and gate metadata after Agents A-C report their exact behavior changes.
- No agent may run `git commit` or `git push`; the coordinator commits after integration and full gates.
- If two agents need the same file, stop that task and report the conflict before editing.

## Task 1: Native Artifact Identity

**Files:**
- Modify: `in-cli/src/native_emit/lower.rs`
- Modify if needed: `in-cli/src/boundary_ir.rs`
- Modify if needed: `in-cli/src/boundary_emit.rs`
- Test: existing `#[cfg(test)]` module in `in-cli/src/native_emit/lower.rs`

- [ ] **Step 1: Write the failing test for ABI identity**

Add a unit test in `in-cli/src/native_emit/lower.rs` near the existing ABI manifest tests. The test should construct a `UnifiedModule` with `identity.package = Some("demo.pkg")` and `identity.module = Some("demo.mod")`, lower it as `NativeLinkage::Dylib`, read the generated `.abi.json`, and assert that the JSON contains both identity values.

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml native_abi_manifest_carries_module_identity --locked
```

Expected before implementation: the test fails because native ABI JSON does not expose package/module identity.

- [ ] **Step 2: Add identity to the native ABI model**

If `BoundaryModule` has no package/module identity fields, add optional fields:

```rust
pub package: Option<String>,
pub module: Option<String>,
```

Populate them in the constructor path used by `boundary_from_module`.

- [ ] **Step 3: Emit identity in ABI JSON**

Update `boundary_emit::emit_abi_manifest` so the ABI manifest includes stable JSON keys:

```json
"package": "demo.pkg",
"module": "demo.mod"
```

Omit the fields when the values are absent, preserving existing manifests for sources without identity.

- [ ] **Step 4: Verify focused native identity tests**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml native_abi_manifest_carries_module_identity --locked
cargo test --manifest-path in-cli/Cargo.toml native_emit --locked
```

Expected: both commands pass.

## Task 2: Swift Subset Parser Depth

**Files:**
- Modify: `in-cli/src/swift_subset.rs`
- Modify if needed: `in-cli/src/native_swift_sil.rs`
- Test: existing `#[cfg(test)]` modules in those files

- [ ] **Step 1: Write failing tests for multiline struct fields and method signatures**

Add Swift subset tests that parse:

```swift
struct Counter {
  let value: Int

  func next() -> Int {
    return value + 1
  }
}

func main() -> Int {
  let c = Counter(value: 1)
  return c.next()
}
```

Assert that `parse_program` returns a `StructDecl` with one field and one method, plus a top-level `main`.

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml swift_subset_multiline_struct_method_subset --locked
```

Expected before implementation: the test fails if the current subset parser cannot retain the method and body shape.

- [ ] **Step 2: Extend parser support only for the bounded shape**

Update `swift_subset.rs` so multiline `struct` bodies can contain:

```swift
let fieldName: Type
func methodName(...) -> Type { ... }
```

Reuse existing `Typ`, `Stmt`, and `Expr` nodes. Do not add full Swift access control, generics, protocols, attributes, macros, or initializers in this wave.

- [ ] **Step 3: Add type/checker coverage**

Extend existing checker logic so method bodies can resolve:

```swift
value
self.value
```

inside the owning struct. Preserve existing diagnostics for unknown functions, unknown identifiers, unknown fields, and return type mismatches.

- [ ] **Step 4: Verify Swift subset path**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml swift_subset --locked
cargo test --manifest-path in-cli/Cargo.toml native_swift_sil --locked
IN_NATIVE_SWIFT_SIL=only ./scripts/check-native-subset-sample.sh
```

Expected: all commands pass without spawning `swiftc` for the native subset sample success path.

## Task 3: `.in` Runtime Error Completion

**Files:**
- Modify: `in-cli/src/lower_core.rs`
- Modify if needed: `in-cli/src/sil_to_bytecode.rs`
- Modify if needed: `in-cli/src/bytecode.rs`
- Test: existing tests in the touched files
- Fixture if needed: `conformance/errors/`

- [ ] **Step 1: Confirm parser support is already present**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml fn_body_parses_throw_statement fn_body_parses_try_catch_statement --locked
```

Expected: parser tests pass. If they fail, fix parser regressions before touching lowering.

- [ ] **Step 2: Write failing VM-level tests for throw and catch**

Add tests that compile `.in` source through Core IR to bytecode and execute:

```in
fn main() -> Int {
  try {
    throw "bad";
    return 1;
  } catch e {
    return 7;
  }
}
```

Expected result: VM exits or returns `7`.

Add a second test:

```in
fn main() -> Int {
  throw "bad";
  return 1;
}
```

Expected result: stable runtime failure diagnostic containing the existing error code or the new code introduced in this task.

- [ ] **Step 3: Implement minimal Core IR lowering for current semantics**

Make `Stmt::Throw` and `Stmt::Try` lower to textual SIL in a way that the bytecode path can distinguish:

```text
throw_error
try_begin
catch_begin
try_end
```

Use the repo's existing instruction style if equivalent markers already exist. Do not implement typed exceptions, stack unwinding across function boundaries, async exceptions, or host-language exception interop in this wave.

- [ ] **Step 4: Implement bytecode/VM behavior**

Add the minimal runtime behavior required by the tests:

- `throw` inside a matching local `try/catch` jumps to the catch body.
- uncaught `throw` returns a stable runtime error.
- normal returns inside the `try` body bypass the catch body.

- [ ] **Step 5: Verify error conformance**

Run:

```bash
cargo test --manifest-path in-cli/Cargo.toml throw try_catch --locked
scripts/check-self-hosted-language-matrix.sh
```

Expected: focused tests pass and the language matrix remains green.

## Task 4: Language Matrix and Roadmap Docs

**Files:**
- Modify: `todo.md`
- Modify: `docs/architecture/conformance-matrix.md`
- Modify: `docs/architecture/parser-surface.md`
- Modify: `docs/architecture/native-swift-master-plan.md`
- Modify if needed: `docs/roadmap-execution-plan.md`

- [ ] **Step 1: Capture live support matrix**

Run:

```bash
in languages --json > /tmp/inauguration-languages.json
scripts/check-self-hosted-language-matrix.sh
```

Expected: matrix command passes. Use `/tmp/inauguration-languages.json` as the source of truth for levels and next steps.

- [ ] **Step 2: Remove stale claims contradicted by code**

Update docs that still say `.in` cannot parse class/interface or try/catch/throw if current parser tests show those are supported. Replace the claim with the narrower remaining gap, such as runtime semantics, VM behavior, or production boundary support.

- [ ] **Step 3: Mark only completed roadmap slices**

In `todo.md`, mark a checkbox complete only if the corresponding implementation and focused tests landed in Tasks 1-3. Leave broad future items open when only one slice was completed.

- [ ] **Step 4: Verify docs and scripts**

Run:

```bash
git diff --check
scripts/check-self-hosted-language-matrix.sh
```

Expected: no whitespace errors and language matrix remains green.

## Task 5: Coordinator Integration and Full Gates

**Files:**
- Review all files changed by Tasks 1-4.

- [ ] **Step 1: Inspect integrated diff**

Run:

```bash
git status --short
git diff --stat
git diff -- in-cli/src/native_emit/lower.rs in-cli/src/boundary_ir.rs in-cli/src/boundary_emit.rs in-cli/src/swift_subset.rs in-cli/src/native_swift_sil.rs in-cli/src/lower_core.rs in-cli/src/sil_to_bytecode.rs in-cli/src/bytecode.rs todo.md docs/architecture/conformance-matrix.md docs/architecture/parser-surface.md docs/architecture/native-swift-master-plan.md docs/roadmap-execution-plan.md
```

Expected: no unrelated files, no generated cache files, no vendor changes.

- [ ] **Step 2: Reinstall local `in`**

Run:

```bash
in update
in doctor
```

Expected: `in doctor` no longer recommends reinstalling the active binary from this checkout.

- [ ] **Step 3: Run required gates**

Run:

```bash
in test
./scripts/check-protocol-models.sh
./scripts/check-native-subset-sample.sh
./scripts/check-in-lang-sample.sh
./scripts/check-icore-sample.sh
git diff --check
```

Expected: all commands pass.

- [ ] **Step 4: Commit and push**

Run:

```bash
git status --short
git add in-cli/src/native_emit/lower.rs in-cli/src/boundary_ir.rs in-cli/src/boundary_emit.rs in-cli/src/swift_subset.rs in-cli/src/native_swift_sil.rs in-cli/src/lower_core.rs in-cli/src/sil_to_bytecode.rs in-cli/src/bytecode.rs todo.md docs/architecture/conformance-matrix.md docs/architecture/parser-surface.md docs/architecture/native-swift-master-plan.md docs/roadmap-execution-plan.md docs/superpowers/plans/2026-06-11-inauguration-parallel-roadmap-wave.md
git commit -m "Advance compiler roadmap wave"
git push
```

Expected: commit and push succeed from the existing branch. Do not create a separate branch.

## Self-Review

- Spec coverage: The plan covers the remaining live surfaces from `todo.md`, `in languages --json`, and stale roadmap docs: `.in`, package/native identity, Swift subset, conformance/docs, and quality gates.
- Placeholder scan: No step delegates vague implementation without a concrete file, command, and expected result.
- Type consistency: The plan uses existing Core IR names: `UnifiedModule`, `Decl`, `Stmt`, `Expr`, `Typ`, `NativeLinkage`, and native ABI manifest paths.
