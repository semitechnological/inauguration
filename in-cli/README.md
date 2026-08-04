# inauguration (`in`)

Rust CLI for the [inauguration](https://github.com/tschk/inauguration) Swift toolchain experiments: embedded hybrid pipeline (AST refresh → frontend → SIL analysis wave), **hotreload daemon** (Unix sockets), SwiftPM staging under `.build/bin` and `.build/artifacts`, plugins, and workspace-only integration commands (`in test`).

Install:

```bash
cargo install inauguration
```

Include the V frontend and other optional Tree-sitter fronts with:

```bash
cargo install inauguration --features extended
```

Full repository layout, benchmarks, Wax/Homebrew installers, and contribution docs live in the GitHub repo.
