- [ ] Make hot reload patch planning graph-aware.
  - Pass structured SIL graph detail into `plan_patch_with_sil_graph` instead of only `Option<u32>` edge counts.
  - Keep downgrade rules conservative: downgrade only when the callee set intersects changed symbols or another explicit dependency signal.
  - Add regression tests in `in-cli/src/hotreload/daemon_impl.rs` for subset graph present, graph unavailable, compile failure, entrypoint files, and non-entrypoint `*App.swift` names.
  - Keep metric `reason` tags precise: `sil_graph=subset`, `sil_graph=subset_zero_edges`, `sil_graph=subset_unavailable`, `sil_graph=non_swift`, and compile-skip cases.

- [ ] Close the Swift graph gap.
  - Add an optional `swiftc` SIL graph path behind an environment flag or explicit CLI mode.
  - Reuse the same source discovery as `sil_emit::combined_swift_sources_for_path` so subset and `swiftc` graph analysis observe the same input set.
  - Record timing and graph source in daemon metrics without increasing default reload latency.

- [ ] Stabilize multi-function SIL analysis.
  - Replace the current "last `sil @...` wins" artifact identity caveat with explicit per-function records.
  - Preserve current merged textual SIL behavior with tests before changing representation.
  - Mirror any `hybrid_sil` changes into `compiler/rust-driver/crates/sil` or add a parity guard so the crates cannot drift silently.

- [ ] Align pipeline timing semantics.
  - Decide whether `total_us` means hybrid wave time, end-to-end pipeline time, or both under separate names such as `wave_us` and `pipeline_us`.
  - Update `in-cli/src/main.rs`, `in-cli/src/hybrid_pipeline.rs`, `compiler/rust-driver/crates/pipeline`, and `compiler/rust-driver/crates/cli` together.
  - Add tests or snapshot expectations for verbose and machine-readable timing output.

- [ ] Add hybrid SIL benchmarks.
  - Add Criterion or an equivalent Rust-native benchmark for `parse_textual_sil`, debug-instruction stripping, and `extract_call_graph`.
  - Cover small subset SIL, multi-function SIL, and representative `swiftc -emit-sil` blobs.
  - Keep outputs useful for `docs/benchmarks` when they affect project direction.

- [ ] Deepen one Tree-sitter front end-to-end.
  - Extend the existing Java path from source to `UnifiedModule` to textual SIL and through `hybrid_sil` graph extraction.
  - Promote another high-value front only after Java has fixture coverage for declarations, bounded bodies, lowering, and diagnostics.
  - Keep parser maturity labels current in `docs/architecture/parser-surface.md`.

- [ ] Grow `.in` beyond v0 declarations.
  - Support multiline struct fields in `in-cli/src/in_lang_parse.rs`.
  - Add statement and expression bodies for `fn`: declarations, assignment, `return`, calls, and simple literals.
  - Lower non-empty Core IR bodies in `in-cli/src/lower_core.rs` instead of emitting only stub SIL.
  - Add fixture tests and update `docs/architecture/in-language.md` as each syntax shape lands.

- [ ] Evolve `.icore` conservatively.
  - Keep `icoreVersion: 1` stable for the current declaration and empty-body shape.
  - Add fixture-driven diagnostics for rejected JSON shapes.
  - Plan the next version only when statement and expression JSON is ready across parser, lowering, docs, and tests.

- [ ] Expand the native Swift subset.
  - Replace header-only parsing with a real subset AST for top-level declarations, struct fields, function signatures, and bounded bodies.
  - Add name resolution, stable diagnostics, and SIL snapshots under `IN_NATIVE_SWIFT_SIL=only`.
  - Make `IN_NATIVE_SWIFT_SIL=try` distinguish unsupported syntax fallback from real subset errors.

- [ ] Reuse native Swift checks in hot reload.
  - Teach `compile_check_cached` to try the native subset path when the environment matches `in build`.
  - Fall back to `swiftc -typecheck` for unsupported Swift.
  - Add cache keys and metrics that include frontend kind, source hash, and fallback reason.

- [ ] Finish `in doctor`.
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
