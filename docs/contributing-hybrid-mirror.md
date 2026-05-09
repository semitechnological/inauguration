# Contributing: hybrid mirror (in-cli ↔ rust-driver)

The **`in`** crate (`in-cli`) vendors **hybrid** pipeline sources that must stay aligned with **`compiler/rust-driver`**. The published **`inauguration`** package is built from `in-cli`; drift between these trees breaks **publish parity** with the workspace compiler (different behavior, duplicated fixes, or CI that passes in one layout but not the other).

Treat edits to either side as edits to **both** until extraction or automation lands (see [native Swift master plan — Phase 0](architecture/native-swift-master-plan.md#phase-0--contracts-and-tooling-12-weeks)).

## Mirrored paths

| `in-cli` source | `compiler/rust-driver` crate (canonical `lib.rs`) |
|-----------------|---------------------------------------------------|
| `in-cli/src/hybrid_core.rs` | `compiler/rust-driver/crates/core/src/lib.rs` |
| `in-cli/src/hybrid_scheduler.rs` | `compiler/rust-driver/crates/scheduler/src/lib.rs` |
| `in-cli/src/hybrid_sil.rs` | `compiler/rust-driver/crates/sil/src/lib.rs` |
| `in-cli/src/hybrid_pipeline.rs` | `compiler/rust-driver/crates/pipeline/src/lib.rs` |

Rust module boundaries inside each crate may split across `src/lib.rs` and submodules in the future; compare **directory trees** and **entry APIs**, not only single files.

## Manual sync checklist

Before opening a PR that touches hybrid logic on **either** side:

1. **Identify the canonical side** for your change (prefer fixing in `compiler/rust-driver` first, then mirror into `in-cli`, unless the change is CLI-specific glue).
2. **Diff the mirrored pairs** after copying or editing. From the repo root, each workspace crate is a single `lib.rs` inlined into the matching `hybrid_*.rs` (expect **`use` path differences**: `hybrid_*` uses `crate::hybrid_*`, crates use `hybrid_*` / workspace deps):

   ```bash
   diff -u compiler/rust-driver/crates/core/src/lib.rs in-cli/src/hybrid_core.rs
   diff -u compiler/rust-driver/crates/scheduler/src/lib.rs in-cli/src/hybrid_scheduler.rs
   diff -u compiler/rust-driver/crates/sil/src/lib.rs in-cli/src/hybrid_sil.rs
   diff -u compiler/rust-driver/crates/pipeline/src/lib.rs in-cli/src/hybrid_pipeline.rs
   ```

   If a crate later splits into submodules, diff the touched subtree against the corresponding section of the vendored file.

3. **Run tests on both workspaces**:

   ```bash
   cd compiler/rust-driver && cargo test --all
   cd in-cli && cargo test
   ```

4. **Confirm `in test`** still passes from the repo root (project gate).

If automated mirroring is added later, keep this checklist as the human fallback when scripts fail or paths diverge.

## Why drift matters

- **crates.io / release binaries** ship **`in-cli`** sources; **`rust-driver`** is the in-tree compiler workspace. Unmirrored fixes can ship to users **without** landing in the driver (or vice versa).
- Reviewers may only glance at one tree; explicit diff discipline reduces silent skew.

For roadmap context and Phase 0 exit criteria, see **[native-swift-master-plan.md](architecture/native-swift-master-plan.md)**.
