# inauguration examples

No language gates. Auto-detect polyglot eval.

## Run examples

```bash
in eval --path examples/polyglot/io.poly       # 4 languages printing
in eval --path examples/polyglot/math.poly     # 9 languages, same expression
in eval --path examples/polyglot/compute.poly   # 3 languages, different math
```

## Auto-detect (no markers)

Separate code blocks with blank lines. Each block auto-detected:

```bash
in eval '
print("hello from python")

console.log("hello from javascript")

println!("hello from rust")
'
```

## Explicit fences (`## lang`)

For ambiguous syntax (same expression in many languages):

```bash
in eval '
## python
2 + 3 * 4
## javascript
2 + 3 * 4
## rust
2 + 3 * 4
'
```

## Capabilities

No levels. Every language lists what it can do:

```bash
in languages             # table: parse/lower/typecheck/boundary/bytecode
in languages --json      # machine-readable
```

## Verify

```bash
bash scripts/verify.sh   # 5 checks, all pass
```
