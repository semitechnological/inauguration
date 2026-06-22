#!/usr/bin/env bash
# Demo: eval in every supported language - math + I/O
IN="${IN_BIN:-$(which in)}"
OK=0 FAIL=0
try() {
  local p="$1" label="$2" code="$3"
  if out=$($IN eval --parser "$p" "$code" 2>/dev/null); then
    echo "  OK $label: $out"
    OK=$((OK+1))
  else
    echo "  FAIL $label"
    FAIL=$((FAIL+1))
  fi
}
echo "=== inauguration language demo ==="
try in  ".in io"    'print("hello from .in")'
try in  ".in math"  '2 + 3 * 4'
try c   "C math"    '2 + 3 * 4'
try cpp "C++ io"      'std::cout << "hello from c++\n";'
try cpp "C++ math"    '2 + 3 * 4'
try objc "ObjC math"  '2 + 3 * 4'
try objcpp "ObjC++ math" '2 + 3 * 4'
try rust "Rust io"    'println!("hello from rust")'
try rust "Rust math"  '2 + 3 * 4'
try zig "Zig io"      'std.io.print("hello from zig")'
try zig "Zig math"    '2 + 3 * 4'
try go "Go io"        'print("hello from go")'
try go "Go math"      '2 + 3 * 4'
try swift "Swift io"  'print("hello from swift")'
try swift "Swift math" '2 + 3 * 4'
try hare "Hare math"  '2 + 3 * 4'
try holyc "HolyC math" '2 + 3 * 4'
try d "D io"          'print("hello from d")'
try d "D math"        '2 + 3 * 4'
try java "Java io"    'System.out.println("hello from java")'
try java "Java math"  '2 + 3 * 4'
try kotlin "Kotlin io" 'println("hello from kotlin")'
try kotlin "Kotlin math" '2 + 3 * 4'
try scala "Scala io"  'print("hello from scala")'
try scala "Scala math" '2 + 3 * 4'
try groovy "Groovy io" 'println("hello from groovy")'
try groovy "Groovy math" '2 + 3 * 4'
try clojure "Clojure math" '2 + 3 * 4'
try csharp "C# io"    'print("hello from c#")'
try csharp "C# math"  '2 + 3 * 4'
try fsharp "F# io"    'print("hello from fsharp")'
try fsharp "F# math"  '2 + 3 * 4'
try vbnet "VB.NET io" 'print("hello from vb.net")'
try vbnet "VB.NET math" '2 + 3 * 4'
try python "Python io" 'print("hello from python")'
try python "Python math" '2 + 3 * 4'
try ruby "Ruby io"    'print "hello from ruby"'
try ruby "Ruby math"  '2 + 3 * 4'
try lua "Lua io"      'print("hello from lua")'
try lua "Lua math"    '2 + 3 * 4'
try perl "Perl io"    'print 42'
try perl "Perl math"  '2 + 3 * 4'
try r "R io"          'print("hello from r")'
try r "R math"        '2 + 3 * 4'
try javascript "JS io" 'console.log("hello from js")'
try javascript "JS math" '2 + 3 * 4'
try typescript "TS io" 'console.log("hello from ts")'
try typescript "TS math" '2 + 3 * 4'
try php "PHP io"      'echo "hello from php"'
try php "PHP math"    '2 + 3 * 4'
try haskell "Haskell math" '2 + 3 * 4'
try ocaml "OCaml math" '2 + 3 * 4'
try elixir "Elixir io" '"hello from elixir"'
try elixir "Elixir math" '2 + 3 * 4'
try erlang "Erlang math" '2 + 3 * 4'
try julia "Julia io"  'print("hello from julia")'
try julia "Julia math" '2 + 3 * 4'
try nim "Nim math"    '2 + 3 * 4'
try crystal "Crystal math" '2 + 3 * 4'
try odin "Odin math"  '2 + 3 * 4'
echo ""
echo "$OK OK · $FAIL FAIL · $((OK+FAIL)) total"
echo ""
echo "=== polyglot mixed-language eval ==="
$IN eval "
## python
print('hello from python')
## rust
println!(\"hello from rust\")
## javascript
console.log('hello from js')
## typescript
console.log('hello from ts')
## zig
std.io.print(\"hello from zig\")
## go
print(\"hello from go\")
## .in
print('hello from inlang')
" 2>&1
echo ""
echo "=== polyglot compute (parallel workloads) ==="
$IN eval "
## python
print(55)
## javascript
console.log(3628800)
## .in
2 + 3 * 4
" 2>&1
