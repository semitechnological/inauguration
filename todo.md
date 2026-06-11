- [x] Prerelease readiness wave for `.in` + general compiler technical preview.
  - Agent JSON cleanup confirmed: `in agent` remains JSON-by-default and does not accept or require `--json`; valid JSON flags remain on `graph`, `package`, `languages`, `explain`, `fix`, and `backend`.
  - Resolved semantic imports now bind to package symbol-index facts in package, graph, and agent reports without installing dependencies or loading extensions.
  - Unresolved semantic imports now produce stable `INPKG001` warning diagnostics in package, graph, and agent-facing reports.
  - `.in` is promoted to Level 3 for its bounded subset only: package/module/import/capability/function diagnostics are source-semantic enough for prerelease agent workflows.
  - Package examples cover both a resolved semantic import and an undeclared semantic import warning.
  - Self-hosting boundary stays honest: owned `.in` / `.icore` / bounded Core IR paths are prerelease-ready, while full self-hosted `.in` compiler replacement remains future work.
  - Acceptance gates: focused package/graph/agent/language tests, sample scripts, `in test`, protocol checks, and diff checks must pass before push.

- [x] Fill owned compiler parity slices for native `.in` aggregates.
  - Optimizer parity landed earlier: Core IR constant folding, dead stores, bytecode peepholes, parameter-store cleanup, and trivial branch cleanup.
  - Multi-front body parity landed earlier: bounded TypeScript, C#, Python, Zig, Kotlin body extraction plus scalar body coverage.
  - Native scalar parity landed: expression statements, bool literals, unary ops, assignment, runtime `if`, `while`, and numeric `match`.
  - Native struct locals, struct methods, string scalars, local arrays, array bounds checks, struct args, struct returns, and array args are implemented and tested.
  - Borrowed array returns landed. Acceptance: `fn identity(xs: [Int]) -> [Int] { return xs; }` compiles native; caller can bind return and index it; array literal return stays rejected to avoid escaping callee stack storage.

- [x] Add native local array indexed assignment.
  - Added indexed assignment AST instead of treating `xs[i] = v` as assignment to raw string `xs[i]`.
  - Typecheck/verifier acceptance: base is `[T]`, index is `Int`, value is compatible with `T`.
  - Native acceptance: `let xs: [Int] = [2,5,8]; xs[1] = 9; return xs[1];` exits `9`.
  - Bounds acceptance: negative and out-of-bounds indexed stores fail like indexed reads.
  - Borrowed array param stores stay deferred until mutability/calling convention is explicit.

- [x] Finish native array ownership ABI.
  - Safe scalar array literal returns now use a static data policy: the native lowerer returns a read-only pointer/length pair for literal arrays instead of exposing callee stack storage.
  - Keep borrowed array returns limited to params/forwarded calls until ownership is explicit.
  - `[Bool]` and `[String]` array args and borrowed returns are covered in native lowering and executable tests.
  - Stable diagnostics for unsupported nested arrays and arrays with aggregate elements now report `native-array-nested-unsupported` and `native-array-aggregate-unsupported`.

- [x] Fill bytecode/Core IR array mutation parity.
  - Lower `Stmt::IndexAssign` from Core IR to bytecode/runtime semantics instead of only native AArch64.
  - Add VM coverage for `xs[i] = v; return xs[i];` through the bytecode compiler.
  - Keep native and bytecode local-array mutation aligned; borrowed/owned array mutation still follows the native ownership TODO.

- [x] Make hot reload patch planning graph-aware.
  - Pass structured SIL graph detail into `plan_patch_with_sil_graph` instead of only `Option<u32>` edge counts.
  - Keep downgrade rules conservative: downgrade only when the callee set intersects changed symbols or another explicit dependency signal.
  - Add regression tests in `in-cli/src/hotreload/daemon_impl.rs` for subset graph present, graph unavailable, compile failure, entrypoint files, and non-entrypoint `*App.swift` names.
  - Keep metric `reason` tags precise: `sil_graph=subset`, `sil_graph=subset_zero_edges`, `sil_graph=subset_unavailable`, `sil_graph=non_swift`, and compile-skip cases.

- [x] Close the Swift graph gap.
  - Add an optional `swiftc` SIL graph path behind an environment flag or explicit CLI mode.
  - Reuse the same source discovery as `sil_emit::combined_swift_sources_for_path` so subset and `swiftc` graph analysis observe the same input set.
  - Record timing and graph source in daemon metrics without increasing default reload latency.

- [x] Stabilize multi-function SIL analysis.
  - Replace the current "last `sil @...` wins" artifact identity caveat with explicit per-function records.
  - Preserve current merged textual SIL behavior with tests before changing representation.
  - Mirror any `hybrid_sil` changes into `compiler/rust-driver/crates/sil` or add a parity guard so the crates cannot drift silently.

- [x] Align pipeline timing semantics.
  - Decide whether `total_us` means hybrid wave time, end-to-end pipeline time, or both under separate names such as `wave_us` and `pipeline_us`.
  - Update `in-cli/src/main.rs`, `in-cli/src/hybrid_pipeline.rs`, `compiler/rust-driver/crates/pipeline`, and `compiler/rust-driver/crates/cli` together.
  - Add tests or snapshot expectations for verbose and machine-readable timing output.

- [x] Add hybrid SIL benchmarks.
  - Add Criterion or an equivalent Rust-native benchmark for `parse_textual_sil`, debug-instruction stripping, and `extract_call_graph`.
  - Cover small subset SIL, multi-function SIL, and representative `swiftc -emit-sil` blobs.
  - Keep outputs useful for `docs/benchmarks` when they affect project direction.

- [x] Deepen one Tree-sitter front end-to-end.
  - Java now has source to `UnifiedModule` to textual SIL to `hybrid_sil` graph coverage.
  - Java method extraction now lowers bounded returns, assignments, and call expressions into Core IR bodies.
  - Dart now shares the generic Tree-sitter scalar body path for params, return types, locals, assignment, calls, `if`, `while`, and returns, with cross-language control-flow fixture coverage.
  - Next high-value promotion: move one remaining level-1 front (`php`, `lua`, `scala`, or `fsharp`) through the same declaration + bounded-body + lowering + diagnostics path.
  - Keep parser maturity labels current in `docs/architecture/parser-surface.md`.

- [x] Add an agent-first compiler mode.
  - Add `in agent` as a fast check/repair mode for coding agents.
  - Keep the CLI command surface and module wiring clean: `in agent`, `in explain`, and `in fix` are single wired commands.
  - Keep the default path parser-compatible with many languages: `.in`, `.icore`, Swift, Rust, Go, V, C, C++, Objective-C, Java, Kotlin, Scala, C#, Python, Ruby, PHP, JavaScript, TypeScript, Zig, Dart, Lua, Elixir, Erlang, Haskell, Julia, R, Nim, D, and Crystal.
  - Emit stable JSON for diagnostics, parser decisions, Core IR summaries, graph facts, effect/capability use, size/timing reports, and repair plans.
  - Make every diagnostic include a code, severity, span, parser id, expected shape, source excerpt bounds, and machine-readable repair hint when one exists.
  - Add `in explain <diagnostic-code> --json` so agents can fetch exact rules without loading long docs.
  - Add `in fix --plan --json` to output typed edits that agents can review and apply without regex-parsing human compiler text.
  - Avoid full backend work by default: parse, type/check, lower enough IR, and stop once the agent has deterministic next-edit facts.
  - Keep human output readable, but treat JSON output as the compatibility contract.

- [ ] Make `.in` the agent-native hybrid language surface.
  - Keep `.in` simple by default: regular syntax, few special cases, explicit imports, explicit fallibility, explicit outside-world capabilities.
  - Let `.in` call or wrap symbols from language fronts that lower into Core IR.
  - First slices landed: top-level `package`, `module`, `import`, `capability`, and `extern <language> fn ...;` declarations parse in `.in`; local relative `.in` imports merge declarations; `std.io` / `std.fs` / `std.http` / `std.json` / `std.process` / `std.cli` synthesize bounded stdlib declarations; extern `requires` contracts warn when capabilities are missing; `if` / `else`, `while`, and binary expressions parse into Core IR; `in agent` and `in graph` report package/module/import facts, capabilities, orchestration facts, and extern call graph edges.
  - `std.env` and `std.path` now synthesize bounded Core IR declarations and agent capability diagnostics; the bytecode VM executes the first self-hosting stdlib subset for `std.fs`, `std.env`, `std.path`, and `std.io`.
  - Next self-hosting slice: grow `apps/in-compiler-bootstrap` from one recognized expression shape into a tiny tokenizer/parser that emits `.icore` for multiple integer expressions.
  - Continue gradual complexity: bind `.in package` / `module` facts to Core IR names and dependency symbols, more standard library APIs, richer expression operators, and additional control flow only when shared IR needs them.
  - Prefer standard library APIs over syntax sugar so agents have one obvious path for files, network, process, JSON, HTTP, and CLI tasks.
  - Keep `icore` as the lowest common interchange format for tools and agents that cannot or should not emit `.in` directly.

- [ ] Unify `.in` identity with the package graph.
  - `idea.md` wants one semantic package graph: package identity, targets, dependencies, capabilities, extensions, indexing, graph invalidation, and semantic imports.
  - First identity slice landed: when a `.in` source declares `package` / `module`, `in graph --json` and `in package --json` report `package_identity` / `source_identity` with stable status and reason codes for match, missing manifest, mismatch, undeclared, unreadable, and non-`.in` sources.
  - Semantic import binding slice landed: top-level `.in` `use database.postgres;` facts parse, resolve against nearest `inauguration.package` dependencies by exact key or dotted suffix, create package symbol-index facts, appear in `in graph --symbols` as dependency symbols, and emit `INPKG001` warnings for unresolved imports without dependency installation or extension loading.
  - Direct calls to resolved dependency symbols now produce `INPKG002` source-semantic warnings so agents see the difference between a known package dependency and a local unknown function.
  - `.in package` and `module` facts now survive in Core IR identity, agent Core IR summaries, graph `package_identity`, package `source_identity`, backend report metadata, and bytecode artifact metadata while preserving unqualified SIL and bytecode function names.
  - Owned native ABI manifests now expose package/module identity for dylib/staticlib artifacts without renaming symbols or changing runtime behavior.
  - Current limitation: `.in package`, `module`, and `use` facts do not affect dependency installation, extension loading, or runtime dependency invocation.
  - Next slice: connect semantic imports to explicit dependency runtime binding while keeping unresolved imports warning-only.

- [x] Build a language-compatibility ladder.
  - Level 0: route extension or magic line to a known `ParserId`.
  - Level 1: extract top-level declarations into `UnifiedModule`.
  - Level 2: lower bounded statements and expressions into Core IR.
  - Level 3: typecheck enough language semantics to produce reliable diagnostics.
  - Level 4: emit graph-aware SIL artifacts and agent repair plans.
  - Level 5: support production build/hotreload semantics for that language family.
  - Publish each language's current level in `docs/architecture/parser-surface.md` and in `in agent` JSON.

- [x] Grow `.in` beyond v0 declarations.
  - Support multiline struct fields in `in-cli/src/in_lang_parse.rs`.
  - Add statement and expression bodies for `fn`: declarations, assignment, `return`, calls, and simple literals.
  - Lower non-empty Core IR bodies in `in-cli/src/lower_core.rs` instead of emitting only stub SIL.
  - Add fixture tests and update `docs/architecture/in-language.md` as each syntax shape lands.

- [x] Evolve `.icore` conservatively.
  - Keep `icoreVersion: 1` stable for the current declaration and empty-body shape.
  - Add fixture-driven diagnostics for rejected JSON shapes.
  - Add `icoreVersion: 2` statement and expression JSON across parser, lowering, docs, and tests.

- [ ] Expand the native Swift subset.
  - First bounded-body slice landed: top-level Swift functions may now carry simple `let`, assignment, call, and return statements into shared Core IR lowering; `main` remains optional for hybrid/library-style sources.
  - Multiline struct fields, bounded instance methods, labeled struct construction, and lowercase receiver method calls now parse/check in the in-tree subset.
  - Continue replacing header-only parsing with a real subset AST for more top-level declarations, richer struct/member forms, function signatures, and bounded bodies.
  - Function-call, local identifier, struct-field, and return-type body checks now emit stable `E_UNKNOWN_FUNCTION` / `E_UNKNOWN_IDENTIFIER` / `E_UNKNOWN_FIELD` / `E_RETURN_TYPE`; `IN_NATIVE_SWIFT_SIL=only` has SIL snapshot coverage for locals, calls, and fields and preserves subset diagnostics before lowering.
  - Bounded `while condition { ... }` bodies now parse into Core IR loops, require Bool conditions via `E_WHILE_COND_TYPE`, and lower through the native SIL subset path.
  - `IN_NATIVE_SWIFT_SIL=try` now falls back only for unsupported non-subset sources and preserves real subset diagnostics such as `E_DUP_TOP`.

- [x] Reuse native Swift checks in hot reload.
  - Teach `compile_check_cached` to try the native subset path when the environment matches `in build`.
  - Fall back to `swiftc -typecheck` for unsupported Swift.
  - Add cache keys and metrics that include frontend kind, source hash, and fallback reason.

- [x] Finish `in doctor`.
  - Check `bash`, `curl`, `cargo`, `swift`, `v`, and the active `in` binary provenance.
  - Report whether `in update` would use the checkout install path or the remote install script.
  - Emit clear remediation for stale installed `in` binaries before `in test`.

- [ ] Keep CI and local gates aligned.
  - Keep `in test` as the required pre-push gate.
  - Keep Linux `in test` on `IN_TEST_SKIP_SWIFT=1` and macOS `in test` full.
  - Add staged skip flags only when a dependency proves too heavy for repeatable local or sandboxed runs.

- [ ] Verification commands before pushing compiler changes.
  - `in update`
  - `in test`
  - `./scripts/check-protocol-models.sh`
  - `./scripts/check-native-subset-sample.sh`
  - `./scripts/check-in-lang-sample.sh`
  - `./scripts/check-icore-sample.sh`
  - `./scripts/bench-swift.sh` and `in bench` when touching benchmarks, hot reload timing, or runtime timing.
