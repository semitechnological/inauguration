(function () {
  var lines = [
    "inside with you",
    "also try in eval",
    "LLVM? never heard of her",
    "40 fronts, one door",
    "mmap goes brr",
    "warm the daemon",
    "icore in your pocket",
    "Tree-sitter ate my braces",
    "MIR means something here",
    "no bytecode, just pages",
    "hello from .in",
    "lower is a lifestyle",
    "JIT-primary don't @ me",
    "crepuscular compilation",
    "sysroot optional trauma",
    "polyglot until bedtime",
    "one Core IR to rule them",
    "parse normalize emit repeat",
    "still faster than cmake",
    "your shebang is valid",
    "capabilities are friends",
    "native emit or bust",
    "in . and chill",
  ];
  var el = document.getElementById("in-splash");
  if (!el) return;
  el.textContent = lines[Math.floor(Math.random() * lines.length)];
})();