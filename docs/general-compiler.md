# General compiler (inauguration)

`in` is a **hybrid compiler driver**: many **source fronts** → shared **Core IR** → **MIR** → **native_emit / JIT**. Scope is compiler infrastructure, agent/graph reports, and owned backends—not UI (that stays in **crepuscularity**).

Status matrix: **`in languages`** / **`in languages --json`**. Roadmap: [universal-compiler-roadmap.md](universal-compiler-roadmap.md).

## Pipeline

```
  .in / .icore / Tree-sitter / rust_front / v_front
                    │
                    ▼
              UnifiedModule (Core IR)
                    │
                    ▼
         compiler::driver → textual SIL / MIR
                    │
                    ▼
           native_emit (AArch64, x86_64) + jit_runtime
```

| Layer | Location | Notes |
|-------|----------|-------|
| Resolution | `parser_registry` | CLI, `IN_PARSER`, shebang, extension → `ParserId` |
| Parse | `in_lang_parse`, `icore`, `tree_front`, … | → `UnifiedModule` |
| Lower | `lower_core`, `mir_lower` | Bounded bodies → machine code |
| CLI | `in-cli` | `build`, `compile`, `execute`, `graph`, `test` |
| Docs hub | `docs-site/` | `crepus web build` / `web serve` (crepuscularity) |

## inlang + crepuscularity

- **inlang** (`.in`): orchestration, capabilities, polyglot driver, JIT programs.
- **crepuscularity** (`.crepus`): declarative UI, `docs-site/` web target (`crepus.toml`), WASM shell.

## icore (JSON Core IR)

- **v1**: declarations only; empty function bodies.
- **v2**: bounded statement/expression bodies for tools.
- **v3**: boundary ABI (`boundary_ir`) for layout/symbol export.

Samples: `apps/icore-sample/`.

## Orchestration surfaces (v0.4)

| Surface | CLI |
|---------|-----|
| Canonicalize | `in canonicalize` |
| Graph | `in graph` |
| Package report | `in package` |
| Backend facts | `in backend` |
| Self-hosted tests | `in test` |

GPU, remote workers, and non-owned runtimes stay **status-only** until in-tree runtime + tests exist. See [orchestration-compiler.md](orchestration-compiler.md), [native-backend.md](native-backend.md).

## Per-language landing

Tree-sitter fronts share scalar body conventions (`return`, `let`, `if`, `while`, calls). Extend each `ParserId` with tests in `in test` / polyglot corpora before raising maturity in `in languages`.

## See also

- [in-language.md](in-language.md)
- [multi-frontend-ir.md](multi-frontend-ir.md)
- [docs-site.md](docs-site.md)