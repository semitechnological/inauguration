# Universal Compiler Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a truthful roadmap and machine-readable language support surface for the universal compiler goal.

**Architecture:** Parser ids and extension routing stay in `in-cli/src/parser_registry.rs`. A new `in-cli/src/language_support.rs` matrix reports language maturity and runtime boundaries, and `in languages` exposes it to humans and agents.

**Tech Stack:** Rust 2024, Clap, Serde, existing `in` CLI, existing Core IR/SIL docs.

---

### Task 1: Track missing requested parser ids

**Files:**
- Modify: `in-cli/src/parser_registry.rs`
- Modify: `in-cli/src/compiler/tree_front/extract.rs`
- Modify: `in-cli/src/agent_mode.rs`

- [x] Add `OCaml`, `Odin`, and `Hare` parser ids.
- [x] Map `.ml`, `.mli`, `.odin`, and `.ha`.
- [x] Keep unsupported fronts level 0 with `.icore` redirects.
- [x] Add routing tests for those extensions.

### Task 2: Add language support matrix

**Files:**
- Create: `in-cli/src/language_support.rs`
- Modify: `in-cli/src/lib.rs`

- [x] Add `LanguageSupport` entries for the requested languages.
- [x] Include parser id, extensions, compatibility level, front, runtime boundary, example, and next step.
- [x] Add tests proving the requested language set is covered.

### Task 3: Expose matrix through CLI

**Files:**
- Modify: `in-cli/src/main.rs`

- [x] Add `in languages`.
- [x] Add `in languages --json`.
- [x] Add a CLI parse test for `--json`.

### Task 4: Write roadmap and spec docs

**Files:**
- Create: `docs/architecture/universal-compiler-roadmap.md`
- Create: `docs/superpowers/specs/2026-05-23-universal-compiler-roadmap-design.md`
- Create: `docs/superpowers/plans/2026-05-23-universal-compiler-roadmap.md`
- Modify: `docs/architecture/parser-surface.md`
- Modify: `docs/architecture/general-compiler.md`
- Modify: `README.md`

- [x] Define the compatibility ladder and runtime policy.
- [x] Link `in languages` as the source of the current support matrix.
- [x] Keep full-language claims scoped to proven levels.

### Task 5: Verify and commit

**Files:**
- All modified files

- [x] Run `cargo fmt --manifest-path in-cli/Cargo.toml`.
- [x] Run `cargo test --manifest-path in-cli/Cargo.toml language_support`.
- [x] Run `cargo test --manifest-path in-cli/Cargo.toml parser_registry`.
- [x] Run `cargo test --manifest-path in-cli/Cargo.toml parse_languages_json_flag`.
- [x] Run `cargo install --path in-cli --locked`.
- [x] Run `in test`.
- [ ] Commit and push after clean gates.
