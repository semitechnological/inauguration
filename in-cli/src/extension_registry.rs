pub const KNOWN_EXTENSIONS: &[&str] = &[
    "distributed-workers",
    "gpu-optimizer",
    "postgres-driver",
    "preview-host",
];

pub fn is_known_extension(name: &str) -> bool {
    KNOWN_EXTENSIONS.contains(&name)
}

pub fn runtime_status(name: &str) -> (bool, &'static str) {
    match name {
        "distributed-workers" => (true, "local-distributed-simulator"),
        "gpu-optimizer" => (false, "gpu-runtime-not-implemented"),
        "postgres-driver" | "preview-host" => (false, "extension-runtime-not-implemented"),
        _ => (false, "unknown-extension"),
    }
}
