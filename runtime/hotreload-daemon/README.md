# hotreload-daemon

Apple-first SwiftUI hot reload daemon.

## Behavior

- File change watcher (planned with `notify`).
- Patch planner chooses `ViewBody`, `Modifier`, or `FullModule`.
- Compatibility gate decides patch vs restart fallback.

## Run

```bash
cargo run -- <watch_root> <socket_path> <metrics_path> <debounce_ms>
```

Example:

```bash
cargo run -- . .brisk/hotreload/daemon.sock .brisk/hotreload/metrics/latest.ndjson 60
```
