# GitHub PR security review — `tschk/inauguration`

**Date:** 2026-07-05  
**Scope:** Open PR inventory + deep review of **#66** (command injection) and **#52** (DoS / non-UTF-8 paths)

---

## Open PRs (limit 35)

| # | Title |
|---|--------|
| 70 | ⚡ Optimize struct init string cloning in lower_core.rs |
| 69 | ⚡ Avoid String Cloning in collect_free_vars Parameter Processing |
| 68 | 🧹 [Remove dead code c_try_return_expr in c_family] |
| 67 | 🧪 Add tests for PackageTargetSelection check methods |
| **66** | **🔒 Fix command injection via unsanitized path in /bin/sh** |
| 65 | 🧪 [improve extract_call_graph test coverage] |
| 64 | 🧪 Add tests for target architecture selection logic |
| 63 | 🧹 Refactor `lower_call_expr` in x86_64_lower.rs |
| 62 | 🧹 Remove unused dead code c_trivial_return_body in c_family.rs |
| 61 | 🧪 [testing improvement description] |
| 60 | 🧪 improve coverage of capability policy validation |
| 59 | 🧪 improve coverage for Core IR integer/float checkers |
| 58 | 🧹 Refactor build_report to extract surface info enrichment |
| 57 | 🧪 add unit test for parse_package_manifest_source |
| 56 | 🧪 Add tests for remove_debug_insts |
| 55 | 🧪 Add test for semantic imports missing dependencies format error |
| 54 | 🧪 Add test coverage for `v_native::parallel::wave_plan` |
| 53 | 🧹 Remove deprecated `from_spec` API from ComponentMetadata |
| **52** | **🔒 Fix DoS vulnerability via unwrap() on non-UTF8 paths** |
| 51 | 🧹 Remove dead code c_try_param_ident_expr |
| 50 | 🧪 Add edge case tests for aggregate benchmark stats |
| 49 | 🧪 Improve test coverage for benchmark regression check |
| 48 | 🧪 Add missing unit tests for `compile_context_at_root` |
| 47 | 🧪 Add test coverage for package lock serialization |
| 46 | 🧪 Add test for language_support_for_parser |
| 45 | 🧪 Add missing error path test for empty lockfile |
| 44 | 🧹 Remove unused path_as_os function |
| 43 | 🧪 Add tests for package lock parser |
| 42 | 🧪 Add `binding_return_type` matching unit tests |
| 41 | 🧪 test: package runtime binding return type |

Other open security-tagged PRs in this window: **#66**, **#52** only.

---

## PR #66 — Fix command injection via unsanitized path in `/bin/sh`

- **URL:** https://github.com/tschk/inauguration/pull/66  
- **Author:** undivisible (+ Jules bot)  
- **Files:** `in-cli/src/native_emit/lower/lower_tests.rs`, `in-cli/src/native_emit/macho.rs`  
- **Diff:** +2 / −6

### What changed

Both sites replaced:

```rust
std::process::Command::new("/bin/sh")
    .arg("-c")
    .arg(path_or_str)  // #66: direct Path in macho; lower_tests still uses to_str in base before PR
```

with:

```rust
std::process::Command::new(path)
```

(`macho.rs` test `roundtrip_answer_code_layout`; `lower_tests.rs` `run_native_exe`.)

### Evidence on `master` (pre-merge)

- `lower_tests.rs:47–49`: `/bin/sh -c` + `path.to_str().unwrap()` — shell interprets argument string.  
- `macho.rs:762–764`: `/bin/sh -c` + `&path` (Path coerced for `-c` still goes through shell parsing of the command string on some platforms; using `-c` with a path string is the anti-pattern).

### Risk assessment

| Topic | Assessment |
|--------|------------|
| **Exploitability in prod** | **Low** — only `#[cfg(all(target_os = "macos", target_arch = "aarch64"))]` test helpers, fixed temp paths (`/tmp/inauguration-macho-roundtrip`, temp dir names). Not user-controlled CLI input. |
| **Severity if path were attacker-controlled** | **High** — `-c` + unsanitized path enables shell metacharacter injection (RCE under shell). |
| **Fix correctness** | **Sound** — `Command::new(path)` executes the binary directly; no shell. Mach-O test already passes `Path` to `codesign` via `.arg(&path)`. |
| **Regression risk** | **Low** — executables are chmod `0o755` and ad-hoc signed; direct exec is the intended pattern on macOS for native emit tests. |

### Verdict: **MERGE** (security hygiene + correct pattern)

**Hold items (non-blocking):**

1. Update stale doc comment in `lower_tests.rs:37` (“execute via /bin/sh”) after merge.  
2. **Overlap with #52:** #52 touches the same `run_native_exe` block but **keeps** `/bin/sh -c` and only fixes UTF-8 `unwrap()` + `Command::arg(path)` for `codesign`/`otool`. After #66, rebase or close #52’s `lower_tests` hunk as superseded for execution path.

### Test gaps

- No new test — change is in test infrastructure only.  
- **Suggested (optional):** none required for merge; existing macOS AArch64 native emit tests exercise `run_native_exe`.  
- No automated test for “malicious path” — acceptable given fixed test paths.

---

## PR #52 — Fix DoS via `unwrap()` on non-UTF-8 paths

- **URL:** https://github.com/tschk/inauguration/pull/52  
- **Author:** undivisible (+ Jules bot)  
- **Files:** `lower_tests.rs`, `native_stdlib.rs`, `owned_compile/tests.rs`  
- **Diff:** +9 / −7  
- **Merge state (gh):** `MERGEABLE`, `UNSTABLE` (CI/checks may be failing — verify before merge)

### What changed

1. **`lower_tests.rs`:** `codesign` and `otool` use `.arg(path)` instead of `path.to_str().unwrap()` in `.args([...])`. **Still runs binary via `/bin/sh -c` with `.arg(path)`** (Path as arg to shell — better than string unwrap, but **does not remove shell**).  
2. **`native_stdlib.rs`:** Non-Unix `instring_from_os_str`: `unwrap_or("")` → `to_string_lossy()`. Tests: `path.to_str().unwrap()` → `path.to_string_lossy()`.  
3. **`owned_compile/tests.rs`:** `Some(out_path.to_str().unwrap())` → `Some(out_path.to_str())` — avoids panic; assertion becomes `Option` vs `Option` (fails cleanly if path not UTF-8).

### Evidence on `master`

Remaining `to_str().unwrap()` in touched areas (grep): `lower_tests.rs` (43, 49, 67), `native_stdlib.rs` (761, 787), `owned_compile/tests.rs` (124).  
Production `artifact_path` is set via `out_path.display().to_string()` (`owned_compile/native.rs`) — already infallible; test-only panic on exotic `TMPDIR`.

### Risk assessment

| Topic | Assessment |
|--------|------------|
| **DoS via panic** | **Real but narrow** — panics only if `Path` is not valid UTF-8 when code forces `to_str().unwrap()`. Attacker must influence path (e.g. `TMPDIR` with non-UTF-8 bytes on Unix). Mostly affects **tests** and **non-Unix** `instring_from_os_str` edge path. |
| **Severity** | **Low–medium** for availability in hypothetical hostile env; **low** for default dev/CI ASCII temp paths. |
| **Fix completeness** | **Partial** — good `Command::arg(path)` and `to_string_lossy()` in stdlib tests; **does not** fix command-injection class in `lower_tests` (still `/bin/sh -c`). |
| **`owned_compile` assertion** | Comparing `report.artifact_path.as_deref()` to `out_path.to_str()` is consistent when paths are UTF-8; if non-UTF-8, `to_str()` is `None` while `display()` may still produce a string — assertion could fail without panic (acceptable). |

### Verdict: **HOLD** — merge only after reconcile with **#66**

| Action | Reason |
|--------|--------|
| **Do not merge as-is ahead of #66** | Leaves `/bin/sh -c` in `run_native_exe`; weaker than #66 for the same lines. |
| **Recommended** | Merge **#66** first, then rebase #52 and **drop** redundant `lower_tests` execution changes; **keep** #52-only hunks: `codesign`/`otool` `.arg(path)`, `native_stdlib.rs`, `owned_compile/tests.rs`. |
| **Alternative** | Close #52 if a follow-up PR combines UTF-8 hardening + #66 execution fix in one diff. |

**Not close** — `native_stdlib` and `owned_compile` changes are still valuable.

### Test gaps

- No test with non-UTF-8 `OsStr` / temp path (would document intended behavior for `to_string_lossy()` vs strict UTF-8 APIs).  
- **Suggested:** optional `#[test]` constructing path from `OsStr::from_bytes` on Unix (skip if platform cannot create file) — low priority.  
- CI: confirm `in test` on macOS AArch64 for `lower_tests` after merge strategy.

---

## Cross-PR summary

```text
                    command injection          UTF-8 panic hardening
PR #66              FIX (no shell)             partial (still unwrap in #66 diff for codesign in lower_tests - check #66 diff again)
```

#66 diff for lower_tests only changes the run line, **not** codesign `path.to_str().unwrap()` at line 43. So after #66 alone, codesign still unwraps.

| PR | codesign/otool Path | run executable |
|----|---------------------|----------------|
| #66 | unchanged (still unwrap on codesign in lower_tests) | `Command::new(path)` ✓ |
| #52 | `.arg(path)` ✓ | still `/bin/sh -c` ✗ |

**Ideal merge order:** #66 (shell removal) + cherry-pick or rebase #52’s non-overlapping hunks (`codesign`/`otool`, `native_stdlib`, `owned_compile`).

---

## Review checklist

| PR | Verdict | Blockers |
|----|---------|----------|
| **#66** | **MERGE** | None; optional doc comment; coordinate with #52 |
| **#52** | **HOLD** | Rebase after #66; remove conflicting `/bin/sh` hunk; verify CI |

---

## Commands run

```bash
gh pr list --state open --limit 35 --json number,title
gh pr view 66 --json ...
gh pr view 52 --json ...
gh pr diff 66
gh pr diff 52
grep /bin/sh in-cli
grep to_str().unwrap() in-cli
```

No repository files modified for this review.