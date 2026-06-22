# inauguration examples

No language gates. Auto-detect polyglot eval.
Eval supports: arithmetic, literals, print/console.log/println!.
For full programs (loops, functions): `in compile --path file.in`.

## Run

```bash
in eval --path examples/polyglot/io.poly       # 4 languages, auto-detected
in eval --path examples/polyglot/compute.poly   # 5 languages, different results
```

## Auto-detect (no markers)

Blank-line separated blocks. Each block auto-detected by content:

```bash
in eval '
print("hello from python")

console.log("hello from javascript")

println!("hello from rust")
'
```

## Explicit fences (`## lang`)

For ambiguous syntax (same expression works in many languages):

```bash
in eval '
## python
2 + 3 * 4
## javascript
42 * 2
## rust
100 + 200
'
```

## Capabilities (no levels)

```bash
in languages             # parse/lower/typecheck/boundary/bytecode
```

## Verify

```bash
bash scripts/verify.sh   # 4 checks, all pass
```
