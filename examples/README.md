# inauguration examples

No language gates. One command: `in eval`.

## Compile + execute (.in files)

Functions, recursion, conditionals. Each file compiles and runs:

```bash
in eval examples/compile/fib.in        # fib(10) = 55
in eval examples/compile/sum.in        # sum_to(100) = 5050
in eval examples/compile/gcd.in        # gcd(48, 18) = 6
in eval examples/compile/even_odd.in   # is_even(42) = true
```

## Polyglot eval (.poly files)

Multiple languages in one file. Auto-detected by content or `## lang` fences:

```bash
in eval examples/polyglot/io.poly      # 4 languages printing
in eval examples/polyglot/compute.poly  # 5 languages, different math
```

## Inline eval

```bash
in eval 'print("hello")'              # inline .in code
in eval '2 + 3 * 4'                   # returns 14
in eval --parser js 'console.log(42)' # specific language
```

## Capabilities (no levels)

```bash
in languages                           # parse/lower/typecheck/boundary/bytecode
```

## Verify

```bash
bash scripts/verify.sh                # 6 checks, all pass
```
