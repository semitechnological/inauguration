# swift-preview-host

Swift actor-based host bridge for hot reload patches.

## Core type

- `PreviewHost` actor applies patch or increments restart fallback counter.

## Validation

```bash
swift build
swift build -Xswiftc -warnings-as-errors
swift test
```

## Client executable

Build and run socket client:

```bash
swift run swift-preview-host-client .brisk/hotreload/daemon.sock
```
