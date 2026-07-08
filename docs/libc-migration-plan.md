# Libc Migration Plan: .in → Inauguration Multi-Language

## Current state

Space kernel includes `kernel/libc.in` (47 functions, 925 lines) that
reimplements C standard library functions in `.in`. The functions cover:

| Category | Functions | Coverage |
|----------|-----------|----------|
| string.h | strlen, strlcpy, strncpy, strcmp, strncmp, strncat, strchr, strrchr, strstr, strdup, strrev | Full |
| memory.h | memset, memcpy, memmove, memcmp | Full |
| ctype.h | isalpha, isdigit, isalnum, isspace, isupper, islower, isprint, toupper, tolower | Full |
| stdlib.h | atoi, atol, itoa, htoa, utoa, fmt_unsigned_base, printf_padded, printf, malloc, calloc, realloc, free, abs, max, min | Partial (no qsort, bsearch, rand) |
| math.h | pow, log10, min, max | Minimal (trig, exp, floor, ceil missing) |
| stdio.h | printf (synthetic) | Very partial (no sprintf, snprintf, sscanf, puts, gets) |
| time.h | time, gmtime, localtime, mktime | Minimal |

**Problem:** This is a maintenance tax. Every bugfix and optimization must be
done twice — once in `.in` and once in the actual C/C++/Rust standard library
for the compilers. The `.in` implementations are also smaller: no locale, no
thread safety, no errno, no signal handling.

## Target state

Compile C/C++/Rust standard library implementations through Inauguration's
multi-language frontend pipeline. The `.in` libc becomes a thin wrapper —
or disappears entirely, with kernel components linking against freestanding
C stdlib functions compiled to Core IR and emitted as SCI components.

## Inauguration's existing multi-language pipeline

```
C source           C++ source         Rust source
  │                   │                   │
  ▼                   ▼                   ▼
Tree-sitter parse   Tree-sitter parse   Tree-sitter parse
  (.c / .h)          (.cpp / .hpp)      (.rs)
  │                   │                   │
  ▼                   ▼                   ▼
extract_c_family    extract_cpp         extract_rust
  │                   │                   │
  └───────────────────┴───────────────────┘
                    ▼
              Core IR (UnifiedModule)
              Decl::Function, Decl::Class, Decl::Component
                    │
                    ▼
          ┌─────────────────┬─────────────────┐
          ▼                 ▼                  ▼
   x86_64 lowering    SCI binary emit    AArch64 lowering
   (x86_64_lower.rs)  (sci.rs)           (aarch64.rs)
          │                 │                  │
          ▼                 ▼                  ▼
   ELF relocatable    Raw SCI binary     Mach-O executable
   object (.o)        (bootable image)   (macOS JIT)
```

**Current C frontend capability:**
- Parse: yes (Tree-sitter C grammar)
- Lower to Core IR: yes (function definitions, struct declarations, typedefs)
- Typecheck: yes
- Native emit (x86_64-unknown-none): yes — `owned-object-subset-freestanding`
- SCI emit: yes — `sci.rs` emitter produces Space Component Images
- **Missing:** pointer type metadata, C ABI boundaries (`boundary` capability flag is absent)

**Current C++ frontend capability:**
- Parse: yes
- Lower to Core IR: yes (classes, methods, fields via `extract_cpp_with_classes`)
- Typecheck: yes
- Native emit: through same pipeline as C

## Migration phases

### Phase 1: Minimal freestanding C string.h

Compile a minimal freestanding C source file through Inauguration's C frontend
to produce a SCI component that Space can load.

**What:**
- Write `libc-minimal/src/string.c` with strlen, strcmp, memcpy, memset
- Compile via `in build --target x86_64-unknown-none --emit sci`
- Load the resulting SCI in Space's kernel instead of calling .in libc versions
- Replace .in calls with SCI component calls via the existing SCI loader

**Status:** Pipeline exists. The C frontend can parse and lower to Core IR.
The SCI emitter can produce raw binaries. The gap is:
1. C pointer types need proper lowering to Core IR pointer types
2. The C frontend currently has `parse + lower + typecheck` but no `boundary`
   capability (C ABI boundary support is not wired)
3. Need to ensure `freestanding` target linkage doesn't require a host libc

**Evidence pipeline works:**
```
in-cli/src/compiler/tree_front/c_family.rs     → C parser
in-cli/src/compiler/tree_front/extract.rs       → shared extraction
in-cli/src/native_emit/sci.rs                  → SCI binary emitter
in-cli/src/native_emit/x86_64_lower.rs         → x86_64 codegen
in-cli/src/native_emit/x86_64.rs               → x86_64 instruction encoding
in-cli/src/native_emit/elf.rs                  → ELF object (for non-SCI path)
```
Tests in sci.rs show the pipeline works for Core IR → SCI binary.

### Phase 2: Freestanding stdlib.h subset

Compile atoi, atol, abs, malloc/free (bump allocator), bsearch/qsort.

**What:**
- Reimplement Space's existing bump allocator in C as a freestanding function
- Replace the .in `malloc`/`free`/`calloc`/`realloc` with the C version
- Verify identical behavior on boot (determinism test already proves repeatability)

**Gaps:**
- malloc/free currently depend on Space's heap (`heap_next`, `heap_end`)
  These are globals in `.in` kernel space. Need a C-ABI boundary to access them.
- The `boundary` capability in the C frontend is not yet implemented.
  This blocks cross-language calls (C calling .in functions and vice versa).

### Phase 3: Full C stdlib on freestanding target

Port musl or a minimal freestanding C library subset through the pipeline.

**Why musl (or similar):**
- musl is designed for static linking and freestanding use
- ~7K lines of .c for a full freestanding subset (vs 925 lines .in currently)
- Battle-tested: bugfixes are done by the musl project, not by Space maintainers
- Single codebase for all languages: C + Rust + C++ all lower to same Core IR

**What's needed from Inauguration:**

| Feature | Status | For |
|---------|--------|-----|
| C pointer type lowering | parse only | memcpy, memmove need pointers |
| C ABI boundary (`boundary`) | not wired | Cross-language calls |
| extern C linkage | not wired | Linking C stdlib with .in kernel |
| Freestanding `__builtin` stubs | not wired | memset/memcpy IA-32 string ops |
| SCI data section globals | ✅ works | String literals, global data |
| x86_64 function body lowering | ✅ works | All C function bodies |

### Phase 4: Rust and C++ stdlib

Once the C pipeline is proven, Rust and C++ standard library implementations
follow the same path. Inauguration already has Rust tree-sitter parsing and
class extraction for C++. The Core IR lowers all three languages to the same
format, so the native emit pipeline needs no changes per-language.

## Key files to change

| File | Change |
|------|--------|
| `in-cli/src/compiler/tree_front/c_family.rs` | Add pointer type lowering, extern linkage, boundary support |
| `in-cli/src/compiler/tree_front/extract.rs` | May need pointer declarator support |
| `in-cli/src/native_emit/sci.rs` | Already works; may need data section alignment fixes |
| `in-cli/src/native_emit/lower/x86_64.rs` | May need C ABI calling convention adjustments |
| `in-cli/src/core_ir.rs` | May need pointer type representation |
| (in Space) `kernel/libc.in` | Remove functions as C replacements are verified |
| (in Space) `kernel/kernel-root.in` | Add SCI libc component loading at boot |

## Verification

Each phase must pass:
1. `in build --target x86_64-unknown-none --emit sci libc-phaseN.c` → produces SCI
2. Load SCI in Space boot → passes determinism test (identical PRNG output)
3. `bash scripts/check-qemu-boot.sh` → full boot verification passes
4. Existing shell commands (ls, cat, ps, etc.) continue working
5. Existing Linux personality shell demo continues working

## Risk assessment

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| C pointer lowering is incomplete | Medium | Phase 1 uses Int for pointers (current .in convention). Add real pointer types in Phase 3. |
| C ABI boundary (`boundary` capability) | Medium | Stick to single-language (C→Core IR→SCI) first. Boundary needed when .in calls C functions. |
| Freestanding linker issues | Low | x86_64-unknown-none ELF object emission already tested with Space kernel. |
| musl has architecture-specific code | Low | Use only scalar subset (no atomics before arch support). |
| SCI loader rejects external components with undeclared caps | Low | Declare `cap_memory()` in component manifest. |
