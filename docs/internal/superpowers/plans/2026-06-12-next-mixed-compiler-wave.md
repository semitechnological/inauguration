# Next Big Mixed Compiler Wave Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Advance `.in` self-hosting, explicit package binding, language support truth, and one frontend gate promotion without adding runtime/plugin execution.

**Architecture:** Keep the wave split by compiler surface. Bootstrap work stays in the executable `.in` fixture plus VM string helpers; package binding stays a reporting contract; language support updates follow measured gates; the frontend promotion only adjusts proven Ruby gate coverage.

**Tech Stack:** Rust 2024 in `in-cli`, `.in` bootstrap source, Bash verification scripts, Cargo, and the repo-installed `in` CLI.

---

## Tasks

- [ ] Add VM/core builtin support for string prefix, index, slicing, and integer detection.
- [ ] Expand `apps/in-compiler-bootstrap/compiler.in` to handle comments, blank lines, whitespace, parentheses, precedence, and structured unsupported-source diagnostics.
- [ ] Add explicit `.in` semantic package binding facts with `bind database.postgres as postgres;`, report them in agent/package/graph surfaces, and suppress `INPKG002` only for explicitly bound calls.
- [ ] Reconcile `.in` language support text and Ruby gate coverage with current repo behavior.
- [ ] Run focused tests, full repo gates, restore incidental lockfile churn, then commit and push.
