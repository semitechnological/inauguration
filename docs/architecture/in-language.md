# `.in` language (v0.2 today, v0.4 orchestration contract)

The **`.in`** front is inauguration’s **brace + line-oriented** compiler/orchestration surface. It shares some minimal-language instincts with the sibling **crepuscularity** project ([`../crepuscularity`](../crepuscularity)), but the ownership boundary is strict: inauguration owns compiler infrastructure, Core IR, backend contracts, orchestration facts, agent reports, and runtime-boundary reporting; crepuscularity owns frontend UI, declarative views, rendering, and cross-platform visual abstraction.

`.in` is shipped here first as something **`in build` can lower to textual SIL** without `swiftc`. It is not a UI language and should not absorb crepuscularity view-tree or rendering responsibilities.

Workflow entry points (flags, sample path, CI script) stay in the repo [README](../../README.md#in-build-and-swiftpm-staging-macoslinux); this page is grammar + IR shape only.

## Ideology

- **Ultraminimal**: top-level declarations first; **v0.2** supports a bounded statement/expression body subset in Core IR plus explicit agent-facing imports, capabilities, and external function bindings.
- **Indent-first on the roadmap**: crepuscularity’s **`.crepus`** files lean indent-first; `.in` v0.2 intentionally accepts familiar **braces + line breaks** so we can reuse the same brace-depth filtering pattern as `native_swift_sil` before tightening the grammar.
- **TS-flavored**: future expression forms can track a small JS/TS-like subset (see crepuscularity README under the repo-root symlink `../crepuscularity`).

## Current behavior (v0.2)

What `in-cli/src/in_lang_parse.rs` implements today:

- Top-level **`import path;`** — accepted as agent-facing source dependency facts. Local relative `.in` imports such as `import "./lib.in";` merge imported declarations into the parsed Core IR module when reading a file. `in agent` exposes imports in the `effects` list as `import:<path>`.
- Standard imports **`std.io`**, **`std.fs`**, **`std.http`**, **`std.json`**, **`std.process`**, and **`std.cli`** synthesize bounded extern-style Core IR declarations for `print`, `read_file`, `write_file`, `http_get`, `json_parse`, `json_stringify`, `process_run`, `arg_count`, and `arg`, with capability requirements checked by `in agent`.
- Top-level **`capability name;`** — accepted as explicit outside-world capability facts. `in agent` exposes them in the `capabilities` list.
- Top-level **`enable name;`** — parsed as an orchestration extension fact and validated against the in-tree extension registry. It does not load plugin/runtime code by itself.
- Top-level annotations **`@pure`**, **`@gpu`**, and **`@parallel_safe`** — parsed as metadata facts and associated with the next function when one follows. They do not execute optimization or GPU scheduling by themselves.
- Top-level **`distributed fn name(...)`** — parsed as a distributed-function orchestration fact and exposed as a local simulated worker job. It does not provide remote workers, retries, or persistence.
- Top-level **`parallel { ... }`** — counted as an orchestration region and lowered to deterministic local plan steps for the calls inside the region. It does not provide threaded runtime execution semantics yet.
- Top-level **`struct Name { … }`** — fields can appear inline or on their own lines between braces. Fields are **`Type fieldName`** segments separated by semicolons or line breaks (e.g. `struct Box { Int x; String label }`). Types must be built-ins or **struct names already declared above** in the file.
- Top-level **`fn name(params) -> Ret`** — **`fn` only** (no `func`, no `function` keyword in v0).
- Top-level **`extern language fn name(params) -> Ret;`** — declares an external binding without embedding a foreign parser. Optional **`requires capability.name`** or comma-separated requirements attach capability contracts; `in agent` warns when a required capability is not declared. The binding lowers as an empty Core IR function declaration so `.in` code can call it and the textual SIL graph can record `function_ref` edges. It is not a foreign runtime call implementation yet.
- Parameters: **`param: Type`** comma-separated.
- Types: **`Int`**, **`String`**, **`Bool`**, **`void` / `Void`** (`void` matching is ASCII case-insensitive), and **named structs** declared above.
- **`fn main`** is required (same spirit as the Swift subset front).
- **Function bodies**: optional brace bodies support `let`, assignment, `return`, `if` / `else`, `while`, binary expressions, call expressions, simple literals, identifiers, and expression statements. Lowering (`in-cli/src/lower_core.rs`) emits bounded textual SIL from non-empty Core IR bodies and records `function_ref` edges for explicit call expressions.
- Nesting: lines inside **`{` … `}`** are ignored for **declaration discovery** when brace-depth ≠ 0 (nested `fn` lines are not top-level), matching the Swift subset filter.

Optional spellings for forward compatibility: **`function`** as an alias may appear later; v0 **does not** accept it. **`Int`** lowers with the same **`Int64`** stub vocabulary as the Swift subset / Core IR `Typ`.

## v0.4 orchestration surfaces

The v0.4 contract is deterministic local visibility and planning before remote execution. `.in` feeds four strict public surfaces:

| Surface | `.in` contribution |
|---------|--------------------|
| Canonicalization | `in canonicalize --path <file> [--check]` parses source through the strict `.in` front and emits deterministic `.in` with normalized types, explicit `-> void`, braced bodies, and semicolon-free statements. |
| Graph command | `in graph --path <file> [--imports] [--capabilities] [--symbols] [--calls] [--json]` reports parser decision, imports/effects, capabilities, symbols, call edges, entry function, orchestration facts, and timing. |
| Package manifest report | `in package --path <dir\|manifest\|source> [--json]` reports package identity, targets, dependencies, capabilities, extensions, package graph nodes, target selection, and capability policy. |
| Orchestration facts | `parse_in_surface_info`, `in agent`, and `in graph --json` expose enabled extensions, annotations, distributed function declarations, parallel region count, local plan steps, local distributed job facts, and explicit runtime reason codes. |

See [orchestration-compiler.md](orchestration-compiler.md) for the command/status contract.

## Runtime boundaries

- **Richer `fn` bodies**: more control-flow forms, richer expression operators, and sharper diagnostics.
- **External / stdlib execution**: today extern declarations and std imports provide Core IR and graph shape; runtime/FFI/plugin invocation is still future work.
- **Parser overrides / discovery**: today **`--parser in`**, **`IN_PARSER=in`**, path **`*.in`**, or magic first-line **`#!in parser=in`** under `--parser auto` select the `.in` front (`in-cli/src/parser_registry.rs`).

GPU execution, remote distributed execution, native machine-code execution, FFI execution, plugin execution, and semantic package installation are **status/contract-only** until backed by in-tree runtime code and tests. Metadata such as `@gpu`, `distributed fn`, `parallel`, `enable`, `extern`, and `requires` may be parsed for diagnostics and graph facts before any runtime execution exists. `distributed-workers` currently means deterministic local worker simulation and planning, not remote scheduling.

Until a feature is implemented in `in_lang_parse.rs`, `lower_core.rs`, the CLI, and the relevant runtime path, treat roadmap bullets as **contracts in progress**, not execution guarantees.

## `hybrid_sil` and merged textual SIL

The pipeline’s `parse_textual_sil` view keeps the legacy “last `sil @…` wins” `SilArtifact::function_id`, but merged blobs now also carry explicit per-function records in `SilArtifact::functions`. `extract_call_graph` uses those records before falling back to instruction-level callers or the legacy single-id behavior. Multi-function emitters (including `lower_to_textual_sil`) still order **`@main` last** so older single-function views stay labeled `main`. See also [multi-frontend-ir.md](multi-frontend-ir.md).

## See also

- [multi-frontend-ir.md](multi-frontend-ir.md) — `UnifiedModule`, parser resolution, SIL caveat.
- [orchestration-compiler.md](orchestration-compiler.md) — v0.4 orchestration/status contract.
- `in-cli/src/in_lang_parse.rs` — parser implementation.
- `in-cli/src/lower_core.rs` — Core IR → textual SIL.
