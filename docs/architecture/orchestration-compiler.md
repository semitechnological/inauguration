# Orchestration compiler contract

`inauguration` is orchestration, backend, and compiler infrastructure. It owns source-front routing, Core IR, textual SIL lowering, graph facts, bytecode execution for the supported subset, agent reports, package/compiler status surfaces, and the hot reload/compiler daemon boundary.

It is not the frontend framework, declarative UI system, rendering layer, or cross-platform view abstraction. Those surfaces belong to the sibling `crepuscularity` project. Crepuscularity can consume inauguration compiler infrastructure, but UI syntax, view trees, rendering, styling, and frontend runtime ownership stay outside this repository unless an integration point is backed by in-tree code and tests.

## Source summary

`idea.md` describes a language and compiler direction centered on semantic intent, graph scheduling, capability systems, AI-agent-readable source, incremental compilation, and orchestration-aware execution. In repo-accurate terms, those ideas map to these current inauguration layers:

| Idea track | Repository-owned surface |
|------------|--------------------------|
| Universal language ingestion | `parser_registry`, `.in`, `.icore`, dedicated Rust/Go/V fronts, Tree-sitter fronts, `UnifiedModule`, Core IR lowering. |
| Compiler-managed orchestration | `compiler/rust-driver`, `hybrid_pipeline`, hot reload planning, SIL graph extraction, timing reports, agent JSON. |
| AI-agent-native source | `.in` imports, capabilities, extern requirements, repair plans, stable diagnostics, Core IR summaries. |
| Backend execution | Textual SIL analysis and the bytecode VM subset where supported. |
| Package and runtime boundaries | `in languages --json`, architecture docs, and explicit runtime-boundary strings per language front. |

Long-range ideas such as distributed workers, GPU execution, native binaries, semantic package installation, and durable task execution are contracts only until the repo contains the runtime implementation, tests, and command output that prove the claim.

## Strict v0.3 surfaces

The v0.3 orchestration surface is deliberately small. Public docs, CLI help, agent JSON, and tests should use these terms rather than broad language-runtime claims.

| Surface | Contract | Current implementation status |
|---------|----------|-------------------------------|
| Canonicalization | A source-to-canonical-source report for `.in` that preserves semantics, produces stable formatting, exposes parser diagnostics, and reports whether output changed. | `in canonicalize --path <file> [--check]` is implemented and covered by `scripts/check-orchestration-compiler.sh`. |
| Graph command | A command surface that reports parser decision, Core IR declarations, SIL functions, call edges, entry function, effects, capabilities, and timing in stable JSON. | `in graph --path <file> [--imports] [--capabilities] [--symbols] [--calls] [--json]` is implemented; text mode prints selected sections and JSON returns the stable fact set. |
| Package manifest report | A machine-readable report for package identity, targets, dependencies, capabilities, extensions, parser fronts, runtime boundaries, and reason codes. | `in package --path <dir\|manifest> [--json]` parses the dependency-free `inauguration.package` shape and reports metadata only. It does not install dependencies or load extensions. |
| Orchestration facts | `.in` surface facts for enabled extensions, annotations, distributed function declarations, and parallel region count. | `parse_in_surface_info` records facts and `in agent --json` reports them with status-only runtime reason codes. |

These four surfaces define v0.3 orchestration visibility. Anything beyond them needs an explicit status field and a reason code instead of implied support.

## Runtime status rules

The compiler may parse or lower syntax before it can execute it. Runtime execution claims require in-tree runtime code and tests.

| Capability | Status until code and tests land |
|------------|----------------------------------|
| GPU tokenization, GPU transforms, GPU scheduling, `@gpu` execution | Status/contract-only. `.in` may parse `@gpu` as metadata, but no GPU runtime execution is owned here. |
| Distributed workers, `distributed fn`, remote scheduling, retries, persistence | Status/contract-only. `.in` may parse distributed function declarations as orchestration facts, but no distributed runtime execution is owned here. |
| Native machine-code execution | Status/contract-only. The current self-hosted path stops at Core IR, textual SIL, SIL analysis, and bytecode VM subset support. See [native-backend.md](native-backend.md). |
| Language runtimes such as JVM, CLR, JavaScript, Python, Ruby, libc, C++ standard library, Dart VM, OCaml runtime | Not bundled unless the repository contains the implementation and tests for the claimed boundary. |

## Command contract shape

New orchestration commands should return stable JSON first and human output second. Reports should include:

- `schema_version`
- `command`
- `status`
- `parser_decision`
- `language_level`
- `source`
- `core_ir_summary`
- `graph_facts`
- `effects`
- `capabilities`
- `orchestration_facts`
- `runtime_boundary`
- `reason_codes`
- `size_timing`

If a surface is not implemented, the report should still be deterministic: `status` should be `contract-only` or `unsupported`, `reason_codes` should name the missing implementation, and no field should imply runtime execution.

## Integration boundaries

- [in-language.md](in-language.md) defines the `.in` grammar facts that can feed this contract.
- [general-compiler.md](general-compiler.md) defines the multi-front Core IR to textual SIL path.
- [universal-compiler-roadmap.md](universal-compiler-roadmap.md) defines language maturity and runtime-boundary rules.
- [native-backend.md](native-backend.md) defines the native execution status contract.
