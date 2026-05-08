use clap::Parser;
use hybrid_core::ChangeEvent;
use hybrid_pipeline::run_wave;
use hybrid_scheduler::BuildScheduler;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "App.swift")]
    path: String,
    #[arg(long, default_value = "App")]
    module_id: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let scheduler = BuildScheduler::default();
    let event = ChangeEvent {
        path: args.path,
        module_id: args.module_id,
        hash: "dev".to_string(),
        timestamp_ms: 0,
    };
    match run_wave(
        &scheduler,
        &event,
        "sil @main\nentry:\ndebug_value %0\n%1 = integer_literal $Builtin.Int64, 1",
    )
    .await
    {
        Ok(count) => println!("processed tasks: {count}"),
        Err(err) => eprintln!("pipeline failed: {err}"),
    }
}
