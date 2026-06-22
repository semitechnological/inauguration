# inauguration examples

No language gates. No explicit language markers needed for basic cases.
Compiler auto-detects syntax: `print(`→ .in, `console.log(`→ JS, `println!(`→ Rust.

## Auto-detect polyglot (blank-line separated)

```bash
# 5 languages, no markers — compiler figures out each block
in eval '
print("hello from python")

console.log("hello from javascript")

println!("hello from rust")

std.io.print("hello from zig")

print("hello from go")
'

# Different arithmetic per language
in eval '
2 + 3 * 4

42 * 2

100 + 200
'
```

## Explicit polyglot (`## lang` fences)

For ambiguous syntax, use `## lang` to disambiguate:

```bash
# 9 languages all computing the same expression
in eval '
## python
2 + 3 * 4
## javascript
2 + 3 * 4
## rust
2 + 3 * 4
...
'
```

## Package ecosystem imports

Import libraries from cargo, npm, pypi, go registries via `use` statements
in `.in` files:

```bash
cd examples/packages
in package install
in compile --path ecosystem.in --out /tmp/eco.bin
in execute-bytecode ecosystem.in --module-id ecosystem_demo.main --verbose
# → String("hono:flask:crepuscularity:fiber")
```

| Ecosystem | Kind | Example import |
|-----------|------|---------------|
| crates.io | `cargo` | `use cargo:serde;` |
| npm | `npm` | `use npm:lodash;` |
| PyPI | `pypi` | `use pip:requests;` |
| Go modules | `go` | `use go:github.com/gin-gonic/gin;` |

## Example files

| File | Mode | What |
|------|------|------|
| `examples/polyglot/io.poly` | auto-detect | 5 languages printing |
| `examples/polyglot/math.poly` | ## fences | 9 languages, same expression |
| `examples/polyglot/compute.poly` | ## fences | 3 languages, different expressions |
| `examples/packages/ecosystem.in` | .in compile | 4 package ecosystem imports |

## Capability table

No levels. Every language advertises its capabilities:

```bash
in languages            # table: parse/lower/typecheck/boundary/bytecode
in languages --json     # machine-readable
```

## Verify everything

```bash
bash scripts/verify.sh  # 7 checks, all pass
```
