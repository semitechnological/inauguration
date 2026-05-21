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
  - Promote another high-value front only after Java-style fixture coverage exists for declarations, bounded bodies, lowering, and diagnostics.
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
  - First slices landed: top-level `import`, `capability`, and `extern <language> fn ...;` declarations parse in `.in`; local relative `.in` imports merge declarations; `std.io` / `std.fs` synthesize bounded stdlib declarations; extern `requires` contracts warn when capabilities are missing; `if` / `else`, `while`, and binary expressions parse into Core IR; `in agent` reports surface facts and extern calls still produce Core IR graph edges.
  - Continue gradual complexity: named package/module resolution, more standard library APIs, richer expression operators, and additional control flow only when shared IR needs them.
  - Prefer standard library APIs over syntax sugar so agents have one obvious path for files, network, process, JSON, HTTP, and CLI tasks.
  - Keep `icore` as the lowest common interchange format for tools and agents that cannot or should not emit `.in` directly.

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
  - Replace header-only parsing with a real subset AST for top-level declarations, struct fields, function signatures, and bounded bodies.
  - Add name resolution, stable diagnostics, and SIL snapshots under `IN_NATIVE_SWIFT_SIL=only`.
  - Make `IN_NATIVE_SWIFT_SIL=try` distinguish unsupported syntax fallback from real subset errors.

- [ ] Reuse native Swift checks in hot reload.
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
