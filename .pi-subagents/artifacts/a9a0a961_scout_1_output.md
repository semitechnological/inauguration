# Code Context — Parser & Type Extension Points (in-cli)

## Files Retrieved

1. `in-cli/src/language_support.rs` (lines 1–435) — static capability matrix per language (`capabilities`, `front`, `next_step`); `language_support_for_parser`, `all_language_support`.
2. `in-cli/src/parser_registry.rs` (lines 1–180, 400–512) — `ParserId`, resolution precedence, `parse_with_resolved` dispatch (In/icore/Rust/boundary fronts vs `tree_front::parse_polyglot_file`).
3. `in-cli/src/compiler/tree_front/mod.rs` (lines 1–32) — polyglot module surface; re-exports `parse_polyglot_file`.
4. `in-cli/src/compiler/tree_front/extract.rs` (lines 1–150, 430–567) — `try_lang_for`, `dispatch`, `parse_lang`, `normalize_entry`, shared `extract_fn_nodes` pattern.
5. `in-cli/src/in_lang_parse.rs` + `in_lang_parse/types.rs`, `validate.rs` — `.in` lexer/parse modules; type parsing; pre-typecheck structural validation.
6. `in-cli/src/typecheck.rs` (lines 71+, 1051–1315) — `TypeChecker`, `typecheck_resolved`, `uses_family_typecheck`, `normalize_module` / `normalize_parser_type`, polyglot entrypoint subset check.
7. `in-cli/src/language_gates.rs` (lines 1–160) — sample-driven gates; `typecheck::typecheck_resolved` for `GATE_SEMANTIC_TYPECHECK`.
8. `in-cli/src/owned_compile/mod.rs` (lines 249–254) — compile path: `desugar_module` then conditional `normalize_module` when `uses_family_typecheck`.
9. `in-cli/src/boundary_capability.rs` (lines 1–40) — JSON view of matrix capabilities + gate reports.

## Key Code

### Resolution → parse (extension point: new `ParserId`)

```452:511:in-cli/src/parser_registry.rs
pub fn parse_with_resolved(
    resolved: ResolvedBuildParser,
    path: &Path,
) -> Result<Option<UnifiedModule>, ParserRegistryError> {
    match resolved {
        ResolvedBuildParser::Swift => Ok(None),
        ResolvedBuildParser::CoreIr(ParserId::In) => in_lang_parse::parse_in_file(path)...
        ResolvedBuildParser::CoreIr(ParserId::Icore) => crate::compiler::icore::parse_icore_file(path)...
        ResolvedBuildParser::CoreIr(ParserId::Rust) => crate::compiler::rust_front::parse_rust_file(path)...
        // Nim, Odin, Hare, D, Crystal, Clojure, VbNet → *boundary* modules
        ResolvedBuildParser::CoreIr(id) => {
            crate::compiler::tree_front::parse_polyglot_file(id, path)...
        }
    }
}
```

### Tree-sitter front (extension point: new grammar)

```58:91:in-cli/src/compiler/tree_front/extract.rs
fn try_lang_for(id: ParserId) -> Option<Language> { ... }
pub fn parse_polyglot_file(id: ParserId, path: &Path) -> Result<UnifiedModule, String> {
    // V special-cased; else dispatch(id, path, &src)
}
```

Per-language logic lives in `compiler/tree_front/{go,java,js,...}.rs`; wired from `extract.rs::dispatch`.

### Typecheck routing (extension point: family vs executable)

```1051:1097:in-cli/src/typecheck.rs
pub fn typecheck_resolved(resolved, module) -> ... {
    if CoreIr(parser_id) && uses_family_typecheck(parser_id) {
        return typecheck_for_parser(parser_id, module);
    }
    typecheck_executable(module)
}
fn typecheck_for_parser(...) {
    let normalized = normalize_module(parser_id, module);
    if uses_polyglot_entrypoint_typecheck(parser_id) { ... } // Lua/JS/TS only
    typecheck_executable(&normalized)
}
```

`uses_family_typecheck` is a **manual** `matches!` list (Php, Lua, Zig, Rust, Java, …) — **not** derived from `language_support.rs`.

### Type normalization (extension point: foreign type names → `Typ`)

```1287:1315:in-cli/src/typecheck.rs
fn normalize_type(typ: &Typ) -> Typ { /* int/i32/long aliases → Int, etc. */ }
fn normalize_parser_type(parser_id: ParserId, typ: &Typ) -> Typ {
    // JS/Python/Ruby/Php: Any → Int hack
    normalize_type(typ)
}
```

`normalize_function_ret` / `normalize_function_body` add implicit returns for Php/Lua/Zig/Scala/Perl/JS/TS.

### Entry naming (parser → typecheck `main` requirement)

```542:547:in-cli/src/compiler/tree_front/extract.rs
pub(super) fn normalize_entry(raw: &str) -> String {
    match raw {
        "Main" => "main".into(),
        other => other.to_string(),
    }
}
```

### `.in` types

```4:26:in-cli/src/in_lang_parse/types.rs
pub(crate) fn parse_in_type(s: &str) -> Typ { ... }
pub(crate) fn parse_param(token: &str) -> (String, Typ) {
    // bad shape → Typ::Named("Unknown")
}
```

Core IR types live in `in-cli/src/core_ir.rs` (`Typ`, `Decl`, `UnifiedModule`); `TypeChecker::check_module` is the single semantic checker for normalized Core IR.

## Architecture

```
CLI / owned_compile / language_gates
    → parser_registry::resolve_parser_id
    → parser_registry::parse_with_resolved
         ├─ in_lang_parse / icore / rust_front / *boundary*
         └─ tree_front::parse_polyglot_file → UnifiedModule
    → lower_core::desugar_module (compile)
    → typecheck::normalize_module (subset of parsers, compile only)
    → typecheck::typecheck_resolved → TypeChecker on Core IR
```

**Three parallel registries** (main maintenance cost):

| Concern | Where |
|--------|--------|
| User-facing capabilities | `language_support::LANGUAGE_SUPPORT` |
| Family typecheck + normalize | `typecheck::uses_family_typecheck` |
| Tree-sitter availability | `extract::try_lang_for` + `dispatch` arms |

Drift examples: **Go, Swift, C, C++, Dart, Groovy, Elixir, Erlang, Haskell, OCaml, Julia, R, F#** claim `typecheck` in matrix (or samples expect gates) but are **outside** `uses_family_typecheck`; **Perl** is in `uses_family_typecheck` but matrix says parse/lower only; **F#** matrix omits typecheck but FSharp is in family list.

## Start Here

Open `in-cli/src/parser_registry.rs` (`ParserId`, `parse_with_resolved`) then `in-cli/src/typecheck.rs` (`typecheck_resolved`, `uses_family_typecheck`, `normalize_module`) — that pair defines what polyglot parse output is actually checked in gates vs compile.

---

## 3–5 Highest-Impact Small Improvements

### 1. Unify typecheck eligibility (gates + compile + docs)

**What:** One function e.g. `parser_id_uses_family_typecheck(id) -> bool` driven by `LanguageSupport::can_typecheck()` (or shared const table used by both matrix and typecheck), replacing duplicate `uses_family_typecheck` match.

**Paths:** `in-cli/src/typecheck.rs` (1063–1089), `in-cli/src/language_support.rs`, tests in `language_support.rs` / `language_gates.rs`.

**Impact:** Fixes semantic-typecheck gate false negatives for Go/Swift/C-family/etc. and false positives/capability lies for Perl/F# without large parser changes.

**Size:** ~50–80 lines + test updates.

### 2. Widen `normalize_type` aliases (systems + JVM leftovers)

**What:** Add lowercase aliases still common in extractors: e.g. `long`, `char`, `byte`, `short`, `uintptr`, `cstring`/`char_ptr`, Go-ish names, `Optional`/`nullable` → keep as Named or map to Void/Any policy.

**Paths:** `in-cli/src/typecheck.rs` (`normalize_type`, ~1287–1305); optional tiny per-`ParserId` table next to `normalize_parser_type`.

**Impact:** Many polyglot samples fail only on return/param `Typ::Named` mismatch after otherwise good parse.

**Size:** Small match arms + 1–2 gate tests per family.

### 3. Expand `normalize_entry` for JVM/CLR/C `main`

**What:** Map `main`/`Main`/`_main` and language-specific static entry symbols to `"main"` where extractors emit PascalCase or mangled names (Java/Kotlin/C# patterns).

**Paths:** `in-cli/src/compiler/tree_front/extract.rs` (`normalize_entry`); if needed one line in `java.rs` / `kotlin.rs` / `csharp.rs` when field name is `main` method.

**Impact:** Direct fix for `missing main function` on otherwise valid samples.

**Size:** ~15–30 lines.

### 4. Fail fast on bad `.in` params (drop `Unknown` type)

**What:** `parse_param` returns `Result` or parse error instead of `Typ::Named("Unknown")` for malformed `name: type` tokens.

**Paths:** `in-cli/src/in_lang_parse/types.rs`, call sites in `decl`/`module` parsing.

**Impact:** Clearer `.in` errors before opaque typecheck failures; tightens native front quality.

**Size:** ~20 lines + parser tests in `in_lang_parse/tests`.

### 5. Document or codegen “new ParserId” checklist (optional 5th: consolidate dispatch)

**What:** Single module comment or `build.rs`-free `docs/architecture/parser-surface.md` section listing: extend `ParserId`, `parser_id_from_extension`, `LANGUAGE_SUPPORT` row, `try_lang_for`+`dispatch` **or** boundary `parse_*_file`, `normalize_parser_type` quirks, gate sample under `apps/polyglot-sample/`.

**Paths:** `docs/architecture/parser-surface.md`, cross-links from `parser_registry.rs` header.

**Impact:** Reduces missed wiring when adding fronts (today easy to parse but never typecheck).

**Size:** Doc-only, or later: thin `ParserFront` trait table (larger).

---

## Suggested priority order

1 → 3 → 2 → 4 → 5 (1 unlocks correct gates for many langs; 3 is tiny with high sample pass rate; 2 is incremental alias work).