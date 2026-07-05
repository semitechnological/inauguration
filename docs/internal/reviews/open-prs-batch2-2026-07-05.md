# Open PR review — batch 2 (2026-07-05)

Excludes merged **#66/#70** and closed **#52/#68/#62/#51**.

## Merge now (low risk)

| PR | Verdict |
|----|---------|
| **#69** | **MERGE** — `collect_free_vars` param refs, no clone semantics change |
| **#65** | **MERGE** — `extract_call_graph` tests |
| **#64** | **MERGE** — `select_backend` unsupported triple tests |
| **#63** | **MERGE** — x86 `lower_call_expr` split; **JIT CI green** on PR |
| **#60** | **MERGE** — capability policy validation tests |
| **#59** | **MERGE** — Core IR int/float checker tests |
| **#57** | **MERGE** — `parse_package_manifest_source` test |
| **#56** | **MERGE** — `remove_debug_insts` tests |
| **#55** | **MERGE** — semantic imports diagnostics test |
| **#53** | **MERGE** — drop deprecated `from_spec` (grep callers first) |
| **#54** | **MERGE** — `wave_plan` tests (if V feature path OK) |
| **#44** | **MERGE** — dead `path_as_os` |
| **#41–50** | **MERGE** batch — package/bench/parser tests |

**Order:** `package_manifest.rs` conflicts — merge **#55 → #57 → #60** before others. `hybrid_sil`: **#56 → #65**.

## Hold / close

| PR | Verdict | Why |
|----|---------|-----|
| **#67** | **CLOSE** | `target_enabled` on wrong type (`PackageTargetSelection` vs manifest); logic ≠ `PackageManifest::target_enabled` |
| **#58** | **HOLD** | Big `build_report` refactor; no tests; wait green `in-cli` + agent report check |

## CI context

Master/local **~18 fails** (macOS native_emit executables, macho). Many PRs show `in-cli` red — may be **baseline**, not PR-only. Trust **JIT conformance** for **#63**.

## Merged on GitHub (2026-07-05)

**#69, #55, #57, #60, #56, #65, #64, #63, #59, #53** (+ prior #66/#70). **#67** closed.

**Still open / blocked:** **#58** (merge conflicts after batch). **#41–54** etc. — merge individually if still open.

Master after pull: **854 pass / 18 fail** (native_emit baseline).

## Suggested commands

```bash
gh pr merge 69 --merge --delete-branch
gh pr merge 55 --merge && gh pr merge 57 --merge && gh pr merge 60 --merge
gh pr close 67 -c "Use PackageManifest::target_enabled / select_targets tests instead."
```