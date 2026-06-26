# Manual Attack Surface Inventory

Generated: 2026-06-25T15:??:??Z
Phase: L4 (Lite Probe)
Target: `/Users/undivisible/projects/inauguration`

---

## Selected Attack-Surface Slices

Based on `knowledge-base-report.md` DFD/CFD slices and threat model priorities:

1. **DFD Slice 2: Package Installation + Runtime** (Supply Chain → Command Execution) — HIGH priority
   - Attackers supply malicious packages → command execution via `package_runtime.rs`
   - Adapter JSON controls `PackageInvokeSpec.program` and `.args`
   - Archive extraction lacks path validation
   - Go module checksum verification skipped for `h1:` prefix
2. **DFD Slice 1: JIT Compilation Pipeline** (Code Generation → Execution) — HIGH priority
   - Unsafe mmap'd executable pages
   - Debug logs leak ASLR layout
   - Function pointer dispatch via inline asm `blr`
   - Error page pointer passed in X27 register

---

## Entry Points

| Entry Point | File | Line(s) | Input | Layer |
|-------------|------|---------|-------|-------|
| `in install` / `in add` | `main.rs` | ~200-340 | CLI args (package refs, version specs) | CLI → Package Installer |
| `in build` / `in run` | `main.rs` | ~700-800 | Source file paths, module IDs, flags | CLI → Compile Pipeline |
| `in compile --target jit` | `main.rs` | ~400-500 | Source code → compiler → JIT | CLI → JIT Runtime |
| `in plugin install` | `main.rs` | ~3560-3600 | Plugin name → bash script | CLI → Subprocess |
| `in plugin run` | `main.rs` | ~3560-3600 | Plugin name + target path | CLI → Subprocess |
| `in test` | `main.rs` | ~3260-3300 | Test command config (program + args) | CLI → Subprocess |
| `in update` | `main.rs` | ~3350-3410 | `IN_INSTALL_DIR` env var, `IN_REPO` env var | CLI → Subprocess |
| `in run` (hotreload daemon) | `main.rs` | ~400-500 | Socket path, watch root, debounce config | CLI → Daemon |
| Hot-reload socket | `daemon_impl.rs` | ~70-100 | NDJSON envelopes | IPC → Daemon |
| Package install (network) | `package_install.rs` | ~810-850 | Registry API URLs, archive content | Network → Package Installer |
| Dynamic module load | `dynamic_module/unix.rs` | ~40-90 | Shared library path | CLI → dlopen |

---

## Public Routes / URLs

| URL Pattern | Method | Purpose | Input Sensitivity |
|-------------|--------|---------|-------------------|
| `https://crates.io/api/v1/crates/{name}` | GET | Cargo registry metadata | Package name (attacker-controlled if published) |
| `https://registry.npmjs.org/{name}` | GET | npm registry metadata | Package name |
| `https://pypi.org/pypi/{name}/json` | GET | PyPI registry metadata | Package name |
| `https://proxy.golang.org/{module}/@v/list` | GET | Go proxy version listing | Module path |
| `https://raw.githubusercontent.com/{repo}/v{version}/install.sh` | GET | Remote install script | Repo slug + version (via IN_REPO env var) |

---

## Attacker Sources (Input Origins)

| Source | Description | Trust Level |
|--------|-------------|-------------|
| CLI arguments (user) | Direct developer input | Developer-controlled |
| Source files (.in, .rs, .swift, 40+ langs) | Files read from filesystem | Developer-controlled |
| Package manifest (inauguration.package) | YAML manifest | Developer-controlled |
| Package adapter JSON (inauguration.adapter.json) | Installed package metadata | **Attacker-controlled** (via malicious package) |
| Installed metadata (inauguration.package.json) | Installed package metadata | **Attacker-controlled** (via malicious package) |
| Registry API responses (HTTPS) | JSON from registry APIs | **Trusted but MITM-able** |
| Registry archive downloads (tar.gz/zip) | Package content via curl | **Attacker-controlled** if registry comproised |
| Dynamic library files (.dylib/.so) | Plugin shared libraries | **Attacker-controlled** (supply chain) |
| NDJSON envelopes (AF_UNIX) | Hot-reload IPC data | **Local attacker** |
| Environment variables (IN_*) | Configuration overrides | Developer-controlled |
| Test command config | Embedded or file-based test specs | Developer-controlled |
| Adapter overlay directory | `adapters/<key>/` filesystem tree | **Attacker-controlled** if write access |

---

## Sinks (Critical Resources)

| Sink | File | Function | Risk |
|------|------|----------|------|
| `Command::new()` + `.args()` | `package_runtime.rs:97` | `run_invoke()` | **Command injection** — args from package metadata |
| `Command::new()` + `.args()` | `main.rs:3264` | test command execution | **Command injection** — test config |
| `Command::new("swift")` | `main.rs:739,745` | `cmd_build()` | External tool invocation |
| `Command::new("bash")` | `main.rs:3408,3564` | `cmd_update_remote`, `cmd_plugin` | Shell execution |
| `Command::new("cargo")` | `main.rs:3354` | `cmd_update` | Build system invocation |
| `Command::new("tar")` | `package_install.rs:623` | `fetch_and_extract()` | **Path traversal** via archive |
| `Command::new("unzip")` | `package_install.rs:691` | `extract_zip()` | **Path traversal** via archive |
| `Command::new("curl")` | `package_install.rs:821,844` | `curl_get()`, `curl_to_file()` | Registry fetch |
| `libloading::Library::new()` | `dynamic_module/unix.rs:47` | `load_dynamic_module()` | **Arbitrary code execution** via dlopen |
| `mmap()` with PROT_EXEC | `jit_runtime.rs` | `CodePage::new()`, `make_executable()` | **JIT code execution** |
| `std::arch::asm!("blr {f}")` | `jit_runtime.rs:138-175` | `invoke()` | **Arbitrary function pointer call** |
| `std::ptr::copy_nonoverlapping()` | `jit_runtime.rs:101` | `load()` | Code page write |
| `sys_icache_invalidate()` | `jit_runtime.rs:63` | `CodePage::finalize()` | Cache flush (macOS) |
| `serde_json::from_str()` | `package_install.rs`, `package_discover.rs`, `daemon_impl.rs` | Various | **Deserialization** of attacker-controlled JSON |
| `fs::write("/tmp/jit_*.log")` | `jit_runtime.rs:118,127` | `invoke()` | **ASLR leak** to world-readable file |

---

## Hidden Control Channels

| Channel | Variable / Parameter | File | Effect |
|---------|---------------------|------|--------|
| ENV: `IN_PARSER` | `in` / `icore` | `parser_registry.rs:341` | Force parser frontend, skip language detection |
| ENV: `IN_TYPECHECK` | `strict` | `bytecode_compiler.rs:34`, `owned_compile.rs:417` | Enable strict typechecking |
| ENV: `IN_NATIVE_SWIFT_SIL` | any non-"only" value | `main.rs:723` | **Controls external toolchain guard bypass** |
| ENV: `IN_SIL_CALLEE_DRIVEN_HOTRELOAD` | `1` | `hotreload/daemon_impl.rs:296` | Enable callee-driven hot reload |
| ENV: `IN_INSTALL_DIR` | path | `main.rs:3359` | Override `cargo install --root` target |
| ENV: `IN_REPO` | `owner/repo` slug | `main.rs:3380` | **Controls remote install script URL** |
| ENV: `IN_PARSER=in` | force `.in` parser | `parser_registry.rs:341` | Any file extension parsed as Core IR |
| ENV: `HOME` | path | `main.rs:3496` | Plugin install directory |

---

## Middleware/Proxy Assumptions

| Assumption | Details | Violation Risk |
|------------|---------|----------------|
| HTTPS enforcement via `require_https()` | `package_install.rs:810` — rejects non-`https://` | **Low** — well-enforced before each curl call |
| No certificate pinning | System CA store via curl | **Medium** — compromised CA can MITM |
| No mTLS | No client certificate for registry fetches | **Low** — registry doesn't require it |
| Package metadata integrity via checksum | SHA-1, SHA-256, SHA-512 or h1: Go sum | **Medium** — SHA-1 broken, Go h1: skipped |
| FNV1a cache integrity | Non-cryptographic hash in `compile_cache.rs` | **Medium** — collision possible |
| ABI version check for dynamic modules | Only `abi_version` validated in `validate_descriptor()` | **High** — pointer_width/endian/layout unchecked |
| External Guard is thread-local + opt-in | `external_guard.rs` — not called by all `Command::new()` sites | **Medium** — audit bypass possible |
| Hot-reload socket file permissions | Assumed local-only via AF_UNIX path permissions | **Medium** — no explicit permission setting |
| Package allowlist (`ALLOWED_INVOKE_PROGRAMS`) | Contains `sh`, `node`, `python3`, `cargo`, `go` | **High** — `sh` allows arbitrary command execution |
| Program allowlist bypass | Path traversal in program name | **Low** — checked via `.contains()` on exact string, not basename normalization |

---

## Key Files (Component Source Paths)

| File | Purpose | Key Risk Areas |
|------|---------|----------------|
| `in-cli/src/jit_runtime.rs` | JIT code execution runtime | Unsafe mmap, debug log ASLR leak, fn ptr dispatch |
| `in-cli/src/package_runtime.rs` | Package export invocation | Command execution via allowlist, args pass-through |
| `in-cli/src/package_install.rs` | Package registry fetch + extract | Archive path traversal, checksum bypass, curl subprocess |
| `in-cli/src/package_discover.rs` | Adapter discovery + ecosystem layout | Adapter JSON parsing, ecosystem-specific command generation |
| `in-cli/src/dynamic_module/mod.rs` | ABI validation | Only checks abi_version, not pointer_width/endian/layout |
| `in-cli/src/dynamic_module/unix.rs` | dlopen loader | Arbitrary code execution via loaded library |
| `in-cli/src/compile_cache.rs` | Compile cache | FNV1a non-crypto hash, cache poisoning |
| `in-cli/src/external_guard.rs` | Tool invocation guard | Thread-local opt-in, not all Command::new() sites covered |
| `in-cli/src/main.rs` | CLI entry point | Test command execution, update/plugin bash, env var injection |
| `in-cli/src/hotreload/daemon_impl.rs` | Hot-reload daemon | AF_UNIX socket without auth, NDJSON parsing |
| `in-cli/src/preview_client.rs` | Hot-reload client | Protocol version check (must be 1) |
| `in-cli/src/native_emit/lower.rs` | Native codegen | Multiple codesign + sh subprocess calls |

---

## Trust Boundary Crossings (Detailed)

| ID | From | To | Data | Validation | Gap |
|----|------|----|------|------------|-----|
| TB-P1 | Registry HTTPS → Package Installer | `package_install.rs` | Archive bytes + metadata JSON | Checksum verify (SHA-1/256/512/h1:) | SHA-1 broken; Go h1: skipped |
| TB-P2 | Installed Package Metadata → Package Runtime | `package_runtime.rs` | `PackageInvokeSpec` (program + args) | Program allowlist; no arg validation | `sh` in allowlist; args passed verbatim |
| TB-P3 | Adapter JSON → Package Discover | `package_discover.rs` | Export bindings (invoke specs) | Parsed from JSON; written to metadata | No signature/authenticity check |
| TB-P4 | Archive tar/zip → Filesystem | `package_install.rs` | Extracted files | `--strip-components=1` | No explicit path traversal prevention |
| TB-J1 | JIT code bytes → Executable memory | `jit_runtime.rs` | Machine code | None at JIT level | No sandbox, no code validation |
| TB-J2 | JIT debug → /tmp/ filesystem | `jit_runtime.rs` | Code page addresses | None | World-readable ASLR leak |
| TB-D1 | Shared library → Process | `dynamic_module/unix.rs` | Loaded code | ABI version check (==1) | No code signing, no path validation |
| TB-C1 | Compile cache metadata → Cache read | `compile_cache.rs` | Cached compile report | FNV1a hash match | Non-cryptographic hash; no signature |
| TB-H1 | AF_UNIX socket → Hot-reload daemon | `daemon_impl.rs` | NDJSON envelopes | protocol_version must be 1 | No auth, no peer credential check |
| TB-M1 | ENV var `IN_REPO` → Install URL | `main.rs:3380` | GitHub repo slug | Slug format validation | No integrity check on install.sh |
| TB-M2 | ENV var `IN_INSTALL_DIR` → cargo install root | `main.rs:3359` | Filesystem path | Trim + parent check | Path traversal via `--root` argument |

---

## Adversary Model (Slice-Specific)

### Package Supply Chain Attacker (DFD Slice 2)
- **Goal**: Execute arbitrary code on developer machine
- **Method**: Publish malicious package with crafted `inauguration.adapter.json` containing `"invoke": {"program": "sh", "args": ["-c", "malicious command"]}`
- **Prerequisites**: Package accepted by registry, developer runs `in install` then imports the package
- **Controls bypassed**: Program allowlist (contains `sh`), args pass-through (no validation)

### Local ASLR Bypass Attacker (DFD Slice 1)
- **Goal**: Read JIT code page base address for exploit chaining
- **Method**: Read `/tmp/jit_invoke.log` after developer runs `in run` with JIT
- **Prerequisites**: Local user access on multi-user machine
- **Controls bypassed**: None — log files are world-readable

### Cache Poisoning Attacker (Compile Cache)
- **Goal**: Substitute compile results
- **Method**: Write to `target/in/cache/<fnv1a-hash>/metadata.json` with controlled content
- **Prerequisites**: Write access to project directory or symlink attack
- **Controls bypassed**: FNV1a hash (non-cryptographic, collision feasible)

---

*End of Manual Attack Surface Inventory*
