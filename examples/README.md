# inauguration examples

inlang is a compiler. One command: `in eval`.
Auto-detects `.in` (compile+execute), `.poly` (polyglot eval), inline code.

## .in — loops, functions, conditionals

Full language: `let`, `while`, `for`, `if`/`else`, `fn`, recursion.

```bash
in eval examples/compile/factorial.in   # 5! = 120 (while loop)
in eval examples/compile/fib.in         # fib(20) = 6765 (while loop)
in eval examples/compile/sum.in         # sum 1..100 = 5050 (for loop)
in eval examples/compile/primes.in      # 10 primes under 30 (nested loops)
in eval examples/compile/collatz.in     # collatz(27) = 111 steps
in eval examples/compile/gcd.in         # gcd(48,18) = 6 (recursion)
in eval examples/compile/even_odd.in    # is_even(42) = true (mutual recursion)
```

## Rust — compile Rust through the same pipeline

```bash
in eval examples/compile/add_multiply.rs  # (10+20)*2 = 60
```

## Polyglot — multiple languages in one file

```bash
in eval examples/polyglot/io.poly         # 4 languages auto-detected
in eval examples/polyglot/compute.poly     # 5 languages, different results
```

## Inline

```bash
in eval 'print("hello")'
in eval '2 + 3 * 4'
in eval --parser js 'console.log(42)'
```

## Capabilities (no levels)

```bash
in languages    # parse/lower/typecheck/boundary/bytecode per language
```

## Verify

```bash
bash scripts/verify.sh   # all examples + capabilities
```
