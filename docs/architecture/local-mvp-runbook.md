# Local MVP Runbook

## 1) Start daemon

```bash
cd runtime/hotreload-daemon
cargo run -- ../../apps/sample-swiftui ../../.brisk/hotreload/daemon.sock ../../.brisk/hotreload/metrics/latest.ndjson 60
```

## 2) Start preview host client

In another terminal:

```bash
cd runtime/swift-preview-host
swift run swift-preview-host-client ../../.brisk/hotreload/daemon.sock
```

## 3) Trigger changes

Edit files in `apps/sample-swiftui/`:

- `ContentView.swift` -> expected compatible patch (`ViewBody`)
- `SampleApp.swift` -> expected incompatible patch (`FullModule`)

## 4) Inspect metrics

```bash
cat ../../.brisk/hotreload/metrics/latest.ndjson
```

Each line includes timestamp, target file, and compatibility result.

## One-command loop

```bash
./scripts/dev-loop.sh
```
