# Balanced Probe Summary — Phase L4

Generated: 2026-06-25T15:??:??Z
Target: `/Users/undivisible/projects/inauguration`
Mode: balanced

---

## Execution Summary

- **Phase**: L4 (Lite Probe Team / Single-Pass)
- **Focus slices**: DFD Slice 2 (Package Installation + Runtime) and DFD Slice 1 (JIT Compilation Pipeline)
- **Files examined**: 12 Rust source files (jit_runtime.rs, package_runtime.rs, package_install.rs, package_discover.rs, dynamic_module/mod.rs, dynamic_module/unix.rs, compile_cache.rs, external_guard.rs, main.rs, preview_client.rs, daemon_impl.rs, lower.rs)
- **Total findings drafted**: 10

## Hypothesis Coverage

| # | ID | Slug | Severity | Slice | Entry Point Verified |
|---|----|------|----------|-------|---------------------|
| 1 | l4-001 | package-runtime-command-injection-via-sh-allowlist | **HIGH** | DFD Slice 2 | Package install → Package runtime |
| 2 | l4-002 | go-module-checksum-bypass-via-h1-prefix | MEDIUM | DFD Slice 2 | Go module install |
| 3 | l4-003 | archive-path-traversal-in-tar-extraction | MEDIUM | DFD Slice 2 | Package archive extraction |
| 4 | l4-004 | jit-runtime-debug-logs-leak-aslr-layout | LOW | DFD Slice 1 | JIT invoke |
| 5 | l4-005 | external-guard-opt-in-bypass | LOW | DFD Slice 2 | External tool tracking |
| 6 | l4-006 | fnv1a-non-cryptographic-cache-key-collision | MEDIUM | Compile Cache | Compile pipeline |
| 7 | l4-007 | dynamic-module-abi-validation-incomplete | MEDIUM | DFD Slice 3 | Dynamic module loading |
| 8 | l4-008 | hotreload-socket-no-authentication | LOW | DFD Slice 1 | Hot-reload IPC |
| 9 | l4-009 | test-command-execution-with-unvalidated-program-args | MEDIUM | CLI | Test runner |
| 10 | l4-010 | in-repo-env-var-controls-remote-install-script-url | LOW | CLI | Update command |

## Key Findings

### HIGH: Package Runtime Command Injection (l4-001)

The most impactful finding. `package_runtime.rs:88` includes `sh` in `ALLOWED_INVOKE_PROGRAMS`. Combined with verbatim `args` passthrough from attacker-controlled installed metadata JSON, this allows arbitrary command execution. A malicious npm/PyPI package with a crafted `inauguration.adapter.json` can execute arbitrary shell commands when the developer imports and invokes the package's export.

**Code path**: Package adapter JSON → `serde_json::from_str()` → `PackageExportBinding.invoke` → `package_runtime.rs:run_invoke()` → `Command::new("sh").args(["-c", "payload"])`.

### MEDIUM: Go Module Checksum Bypass (l4-002)

`verify_archive_checksum()` at `package_install.rs:662-669` skips actual hash verification for `h1:` prefix Go module checksums, relying on the assumption that "Go already verified during download." Cache copies via `file://` prefix bypass Go's verification entirely.

### MEDIUM: FNV1a Cache Collision (l4-006)

The compile cache uses non-cryptographic FNV1a for cache key derivation. Collision attacks are computationally feasible. Attacker with write access to `target/in/cache/` can substitute compile results.

### MEDIUM: Dynamic Module ABI Bypass (l4-007)

`validate_descriptor()` only checks `abi_version`. The `pointer_width`, `endian`, and `layout_hash` fields are read but never validated against expected values, allowing ABI-mismatched modules to load.

## Coverage Assessment

| Attack Surface Area | backward-reasoner | contradiction-reasoner | Verified |
|--------------------|:-:|:-:|:-:|
| Package Runtime Command Execution | ✅ | ✅ | ✅ file:line evidence |
| Go Module Checksum | ✅ | ✅ | ✅ file:line evidence |
| Archive Path Traversal | ✅ | ✅ | ✅ file:line evidence |
| JIT Debug ASLR Leak | ✅ | ✅ | ✅ file:line evidence |
| External Guard Coverage | ✅ | ✅ | ✅ file:line evidence |
| FNV1a Cache Poisoning | ✅ | ✅ | ✅ file:line evidence |
| Dynamic Module ABI | ✅ | ✅ | ✅ file:line evidence |
| Hot-reload Socket Auth | ✅ | ✅ | ✅ file:line evidence |
| Test Command Injection | ✅ | ✅ | ✅ file:line evidence |
| IN_REPO Install Script | ✅ | ✅ | ✅ file:line evidence |

All 10 hypotheses were verified with code-level evidence using `read`/`grep` for exact file:line references.

## Areas Not Covered

- **Tree-sitter parser C FFI** (39 grammar parsers): Out of scope for L4 — requires C-specific static analysis and fuzzing.
- **native_emit/lower.rs codegen bugs**: 169KB file with complex machine code generation. Requires domain-specific code analysis tools.
- **Scripts directory** (42 shell scripts): L4 focused on Rust source; shell scripts in `scripts/` may have their own vulnerabilities.
- **Compiler driver sub-crates** (`compiler/rust-driver/`): Separate workspace with 7 sub-crates — not in-scope for this probe.

## Recommended Follow-ups

1. **Immediate**: Remove `sh` from `ALLOWED_INVOKE_PROGRAMS` (l4-001). This is the highest-impact, lowest-effort fix.
2. **Short-term**: Add path validation after archive extraction (l4-003). Implement proper `h1:` hash verification for Go modules (l4-002).
3. **Medium-term**: Replace FNV1a with SHA-256 in compile cache keys (l4-006). Add code signing or path validation for dynamic modules. Add peer credential verification to hot-reload socket.
4. **Low priority**: Remove JIT debug log writes in release builds (l4-004). Move `ExternalInvocationGuard` from opt-in to always-recording.
