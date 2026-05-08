# Vendored Swift compiler (`vendor/swift`)

The directory **`vendor/swift/`** is **gitignored**. Keep a full Swift compiler checkout there when you want **`in`** to drive **your** `swiftc` instead of the Xcode / toolchain copy on `PATH`.

## What inauguration changes

Patch file: **`patches/vendor-swift/inauguration-emit-sil-stdout.patch`**

It adds a hidden **frontend-only** flag:

- Pass through **`swiftc`** as **`-Xfrontend -inauguration-emit-sil-to-stdout`**.
- After Swift’s normal SIL processing for **`-emit-sil`**, canonical SIL is printed to **stdout** instead of only to the **`-o`** file.

`in-cli` turns that on when **`IN_SWIFT_EMIT_SIL_STDOUT=1`** (see **`in-cli/src/sil_emit.rs`**). **Stock `swiftc` fails** with `unknown argument: '-inauguration-emit-sil-to-stdout'` — enable this env var only after applying the patch. If the patched compiler exits successfully but prints nothing to stdout, **`in`** still tries to read the temp **`-o`** SIL file.

## Apply the patch

From the inauguration repo root:

```bash
./scripts/apply-vendor-swift-inauguration-patch.sh
```

Or with a checkout elsewhere:

```bash
IN_VENDOR_SWIFT_ROOT=/path/to/swift ./scripts/apply-vendor-swift-inauguration-patch.sh
```

## Build Swift (outline only)

Follow Swift’s own docs for your OS (dependencies, CMake/Ninja, presets). After you have a `swiftc` binary from that build:

```bash
export IN_SWIFTC=/path/to/build/swift-macosx-arm64/bin/swiftc   # example layout; yours may differ
export IN_SWIFT_EMIT_SIL_STDOUT=1    # optional; uses stdout SIL path when patched
in build --path … --module-id …
```

The compiler remains Swift.org code plus this small hook; **`in`** still owns SIL passes and scheduling after textual SIL is produced.
