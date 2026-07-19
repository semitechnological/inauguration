# Freestanding `thumbv8m.main-none-eabi`

Generic Cortex-M33 freestanding target support in Inauguration.

## Status

| Piece | Status |
| --- | --- |
| Target registry identity | Implemented |
| Native-emit registry | Implemented (`elf32-relocatable-object+raw+uf2`) |
| ELF32 relocatable object (const scalar return) | Implemented |
| Thumb-2 return stub | Implemented (`movs r0, #imm; bx lr`) |
| Raw binary helper | Existing `native_emit::raw` |
| UF2 packing helper | Implemented (`native_emit::uf2`) |
| Linker layout script generator | Implemented (`native_emit::linker_layout`) |
| Full Thumb instruction lowering | Not yet |
| Vector table / IRQ synthesis | Product-owned (e.g. Subspace) |
| MPU / board capsules | Product-owned |

## ABI contract (MVP)

- freestanding, no libc, no heap
- integer return values only for the owned object subset
- entry symbol name supplied by caller (`--entry`)
- ELF machine `EM_ARM` (40), class ELF32, little-endian
- Thumb instruction encoding for the stub body

Foreign application bodies (C/Zig) must still pass product-level freestanding
contract checks (stack, alloc=none/pool, blocking bounds). Inauguration only
emits the object/raw/UF2 bytes once those contracts are verified upstream.

## Example

```bash
in compile \
  --path answer.in \
  --target native \
  --target-triple thumbv8m.main-none-eabi \
  --linkage static-lib \
  --entry answer \
  --out answer.o \
  --json
```

## Ownership

Inauguration owns generic emit helpers. Board bases, SCI-E, resource ledgers,
and RTOS runtime stay in product repositories such as Subspace.
