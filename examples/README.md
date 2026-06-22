# inauguration examples

No language gates. The compiler auto-detects syntax:
`print(` → inlang, `console.log(` → JavaScript, `println!(` → Rust.

## Auto-detect polyglot (blank lines separate languages)

```bash
in eval '
print("hello from python")

console.log("hello from javascript")

println!("hello from rust")

std.io.print("hello from zig")

print("hello from go")
'
```

Each paragraph is auto-detected by language and evaluated independently.

## Explicit polyglot (`## lang` fences)

For ambiguous syntax (like `2 + 3 * 4` which works in many languages),
use `## lang` to disambiguate:

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

## Example files

```bash
in eval "$(cat examples/polyglot/io.poly)"      # 5 languages printing
in eval "$(cat examples/polyglot/math.poly)"    # 9 languages, same math
in eval "$(cat examples/polyglot/compute.poly)"  # 3 languages, different math
```

## Capability table

No levels. Every language advertises what it can do:

```bash
in languages            # parse | lower | typecheck | boundary | bytecode
in languages --json     # machine-readable
```

## Verify everything

```bash
bash scripts/verify.sh  # 5 checks, all pass
```
