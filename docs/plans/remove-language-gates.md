# Language Gate Removal Plan

**Goal:** Replace the static 0–5 level/gate system with dynamic capability checks.
Any language can be used for any operation — the compiler queries "can you do X?"
rather than "are you level ≥ 4?". Mixing Rust + Python + TypeScript in one
module stream just works.

**Status:** Planning only. No code changes yet.

---

## 1. What exists today

### 1.1 Three interlocking files

| File | Lines | Role |
|------|-------|------|
| `language_support.rs` | 470 | Declares 40 language entries, each with `level: u8` (2–5) |
| `language_gates.rs` | 423 | 11 gates, sequential evaluation, produces `evaluated_level` |
| `boundary_capability.rs` | 272 | Wraps gate reports into boundary/effective/evaluated triples |

### 1.2 The 11 gates

```
reported → core-ir-decls → core-ir-bodies → textual-sil →
semantic-typecheck → boundary-ir-verify → boundary-ir-attach →
boundary-extract → abi-layout-hash → abi-emit → bytecode-vm
```

Each gate requires the previous one. A language at level 3 has proven
it passes gates 1–5. The test `all_languages_are_level_four_or_five()`
blocks CI if any language drops below 4 (except Perl, hardcoded skip).

### 1.3 Gate evaluation flow

```
polyglot_sample_for(lang) → find sample file → parse → check bodies
→ lower to SIL → family typecheck → load boundary → verify boundary
→ emit ABI → bytecode compile → assign level
```

### 1.4 Consumers of the level number

| Consumer | What it uses `level` for |
|----------|--------------------------|
| `all_languages_are_level_four_or_five()` test | CI gate — blocks if level < 4 |
| `boundary_capability_for()` | Computes `effective_level = min(boundary, declared)` |
| `cmd_languages --json` in `main.rs` | Display `level`, `boundary_level`, `effective_level` in JSON |
| `declared_level_never_exceeds_evaluated` test | Asserts static level ≈ measured level |
| `level_five_fronts_include_dedicated_fronts` test | Asserts in/icore/V/JS/TS/Odin are level 5 |
| README language support table | Shows 0–5 level numbers to users |

### 1.5 What the user asked for

> "I don't think we should have certain language 'gates' — we should take each
> thing as it comes. If somebody mixes Rust and Python or TypeScript together,
> it should just work."

Translation: remove the level number. The compiler should check "can this
language provide a core IR body right now?" not "has this language passed
gate 3 in a separate test run?"

---

## 2. What to change

### 2.1 Phase A: Remove static levels from `LanguageSupport`

**File:** `in-cli/src/language_support.rs`

| Remove | Reason |
|--------|--------|
| `level: u8` field | Replaced by capabilities list |
| `level_label` field (keep, rename to `capabilities`) | Same info, no rank |
| `all_languages_are_level_four_or_five()` test | Blocks things for wrong reason |
| `level_five_fronts_include_dedicated_fronts()` test | Tests the level number, not behavior |
| `ruby_reports_bounded_body_lowering()` test | Asserts level == 4 |
| `javascript_and_typescript_report_bounded_bytecode_vm_level()` test | Asserts level == 5 |

| Add | Reason |
|-----|--------|
| `LanguageSupport.capabilities: &'static [&'static str]` | List what this frontend CAN do: `["parse", "lower", "typecheck", "boundary", "bytecode"]` |
| `fn can_parse(&self) -> bool` | One-liner checked at pipeline entry |
| `fn can_lower(&self) -> bool` | One-liner for core IR extraction |
| `fn can_boundary(&self) -> bool` | One-liner for cross-language FFI |

Keep `runtime_boundary`, `front`, `next_step` — they're descriptive, not
normative. Rename `level_label` to `capabilities` in the struct but keep
the same text (it's useful).

### 2.2 Phase B: Simplify `language_gates.rs` → `language_capabilities.rs`

**File:** `in-cli/src/language_gates.rs` (rename to `language_capabilities.rs`)

| Delete | Reason |
|--------|--------|
| 11 `GATE_*` constants | Replaced by capability strings |
| `evaluated_level: u8` field | Levels gone |
| `level_from_gates()` | Levels gone |
| `finish_level()` | Merge into single `finish()` |
| `evaluated_boundary_level()` | No longer needed |
| Sequential gate ladder logic | Each capability checked independently |

| Replace with | Reason |
|--------------|--------|
| `check_capabilities(path) -> CapabilitySet` | Returns `{parse: true, lower: true, boundary: false, ...}` |
| `CapabilitySet` struct | Boolean map of what this language/source can do |
| `polyglot_sample_for()` stays | Sample files still useful for CI verification |

The capability checks run the same underlying functions (parse, lower,
typecheck, boundary, bytecode) but report them independently instead of
aggregating into a level. A language can pass "parse" and "typecheck" but
fail "boundary" — the compiler now knows to use it for eval but not for
FFI, instead of blocking everything at level 2.

```rust
// Before
let report = evaluate_language_gates(entry, root);
if report.evaluated_level >= 4 { ... }

// After
let caps = check_capabilities(sample_path);
if caps.boundary { ... }
if caps.bytecode { ... }
```

### 2.3 Phase C: Simplify `boundary_capability.rs`

**File:** `in-cli/src/boundary_capability.rs`

- Remove `boundary_level`, `effective_level`, `evaluated_level` triples
- `BoundaryCapability` → `CapabilitySet` (every field is `bool`)
- `language_support_json()` returns capabilities directly, no level numbers
- All tests rewritten to check `caps.parse`, `caps.lower`, etc.

### 2.4 Phase D: Update consumers

| File | Change |
|------|--------|
| `main.rs` `cmd_languages` | Display capabilities, not levels |
| `README.md` | Replace level table with capability table |
| All `#[test]` functions | Check capabilities, not level numbers |

---

## 3. Cross-language interop (the real goal)

After gates are gone, the compiler pipeline can query per-language
capabilities at each stage:

```
Source: main.in (imports python.py, typescript.ts)
→ Parse: in ✓ python ✓ typescript ✓
→ Lower: in ✓ python ✓ typescript ✓
→ Typecheck: in ✓ python ✓ typescript ✓
→ Boundary: in ✓ python ✗ typescript ✓
→ Bytecode: in ✓ typescript ✓
→ Output: in + typescript bytecode, python lowered to textual SIL only
```

No concept of "level" — each language contributes what it can. The pipeline
stops at the first stage where a needed language fails, with a clear error:
`python cannot produce boundary IR: python_boundary module not found`.

---

## 4. What NOT to change

- `polyglot_sample_for()` — keep it, sample files are useful for CI
- The actual gate evaluation functions (`evaluate_path`, `load_boundary`,
  `module_has_bodies`) — they still run, just return `CapabilitySet` instead
  of aggregate level
- `parser_registry`, `extract.rs`, all tree-sitter frontends — unchanged
- `core_ir.rs`, `family_typecheck.rs`, `bytecode_compiler.rs` — unchanged

---

## 5. Execution order

| Step | Files | ~Lines changed | Risk |
|------|-------|----------------|------|
| 1. `LanguageSupport` drop `level`, add `capabilities` | `language_support.rs` | +30, -20 | Low |
| 2. Rename `language_gates.rs` → `language_capabilities.rs`, remove levels | `language_gates.rs` → renamed | +50, -150 | Low |
| 3. `CapabilitySet` struct, independent checks | `language_capabilities.rs` | +40, -30 | Low |
| 4. Update `boundary_capability.rs` | `boundary_capability.rs` | +20, -50 | Medium |
| 5. Update `cmd_languages` in `main.rs` | `main.rs` | +10, -10 | Low |
| 6. Update README | `README.md` | +15, -40 | None |
| 7. Run `in test` + `cargo test`, fix failing tests | various | ~20 | Medium |

**Total:** ~+185, ~−300 lines net −115.

---

## 6. Ponytail gut-check

- **Does this need to exist?** Yes — the user explicitly asked for cross-language
  interop and removing the level number system.
- **Laziest approach:** Make `evaluated_level` always return maximum, keep
  the structs, skip the refactor. Nope — user wants real removal.
- **Actually shipped:** Phases A–D as one commit. No new abstractions. One
  `CapabilitySet` bool-map struct. `polyglot_sample_for()` unchanged.
  Sample files still checked in CI, just not gated by a single number.
- **Skipped:** No `LanguageFrontend` trait, no plugin architecture for
  capabilities, no config file for per-language capabilities. Add when
  someone writes a 41st language frontend.
- **Tests:** Each removed test gets a capability-based replacement.
  `all_languages_are_level_four_or_five()` becomes
  `all_languages_can_parse_and_lower()`.

---

## 7. Post-refactor capability table (draft)

| Language | parse | lower | typecheck | boundary | bytecode |
|----------|-------|-------|-----------|----------|----------|
| in | ✓ | ✓ | ✓ | ✓ | ✓ |
| icore | ✓ | ✓ | ✓ | ✓ | ✓ |
| JavaScript | ✓ | ✓ | ✓ | ✓ | ✓ |
| TypeScript | ✓ | ✓ | ✓ | ✓ | ✓ |
| Odin | ✓ | ✓ | ✓ | ✓ | ✓ |
| V | ✓ | ✓ | ✓ | ✓ | ✓ |
| Rust | ✓ | ✓ | ✓ | ✓ | — |
| Zig | ✓ | ✓ | ✓ | ✓ | — |
| Go | ✓ | ✓ | ✓ | — | — |
| Swift | ✓ | ✓ | ✓ | — | — |
| Python | ✓ | ✓ | ✓ | — | — |
| Ruby | ✓ | ✓ | ✓ | — | — |
| PHP | ✓ | ✓ | ✓ | — | — |
| Lua | ✓ | ✓ | ✓ | — | — |
| Scala | ✓ | ✓ | ✓ | — | — |
| Kotlin | ✓ | ✓ | ✓ | — | — |
| Java | ✓ | ✓ | ✓ | — | — |
| C# | ✓ | ✓ | ✓ | — | — |
| C | ✓ | ✓ | ✓ | — | — |
| C++ | ✓ | ✓ | ✓ | — | — |
| ObjC | ✓ | ✓ | ✓ | — | — |
| ObjC++ | ✓ | ✓ | ✓ | — | — |
| Dart | ✓ | ✓ | ✓ | — | — |
| Perl | ✓ | ✓ | — | — | — |
| Nim | ✓ | ✓ | ✓ | ✓ | — |
| D | ✓ | ✓ | ✓ | ✓ | — |
| Crystal | ✓ | ✓ | ✓ | ✓ | — |
| Hare | ✓ | ✓ | ✓ | ✓ | — |
| Clojure | ✓ | ✓ | ✓ | ✓ | — |
| VB.NET | ✓ | ✓ | ✓ | ✓ | — |
| OCaml | ✓ | ✓ | ✓ | — | — |
| F# | ✓ | ✓ | — | — | — |
| Haskell | ✓ | ✓ | — | — | — |
| Elixir | ✓ | ✓ | — | — | — |
| Erlang | ✓ | ✓ | — | — | — |
| Julia | ✓ | ✓ | — | — | — |
| R | ✓ | ✓ | — | — | — |
| Groovy | ✓ | ✓ | ✓ | — | — |
| HolyC | ✓ | ✓ | — | — | — |

> Tentative mapping from current gate system. Actual capabilities verified
> per-language by `check_capabilities()` at build/test time.

---

## 8. What mixing languages looks like (after)

```bash
# Today: needs --parser flag, one language at a time
in eval '2 + 3 * 4'                 # .in only

# After: auto-detect, mix freely
in eval --polyglot '
  // typescript part
  const x = 2 + 3;
  print(x);
  
  # python part
  y = x * 4
  print(y)
  
  // rust part
  let z = y + 10;
  println!("{}", z);
'
```

Each expression block runs its own frontend, shares variables through
Core IR, emits unified bytecode. This is Phase 2 work (not part of this
refactor plan) — this plan only removes the static level barrier so Phase 2
is possible.
