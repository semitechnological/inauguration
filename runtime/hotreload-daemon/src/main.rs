use hotreload_daemon::{DaemonConfig, run_daemon};
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("hotreload-daemon error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let watch_root = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let socket_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| ".brisk/hotreload/daemon.sock".to_string()),
    );
    let metrics_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| ".brisk/hotreload/metrics/latest.ndjson".to_string()),
    );
    let debounce_ms = args
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(60);
    let config = DaemonConfig {
        watch_root,
        socket_path,
        metrics_path,
        debounce_ms,
    };
    run_daemon(config).await?;
    Ok(())
}
