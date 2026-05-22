mod alert;
mod config;
mod monitor;
mod state;
mod ui;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "tortuga", about = "HTTP endpoint monitor")]
struct Args {
    /// Path to the TOML config file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = config::load(&args.config)?;

    // Load persisted state from disk (best-effort)
    let state_path = PathBuf::from(&cfg.global.state_file);
    let initial_state = state::load_state(&state_path).unwrap_or_default();
    let shared_state: state::SharedState = Arc::new(Mutex::new(initial_state));
    let shared_log: state::SharedLog = Arc::new(Mutex::new(Vec::new()));

    // Shared HTTP client and global config
    let client = reqwest::Client::new();
    let global = Arc::new(cfg.global.clone());

    // Spawn one monitor task per endpoint
    let handles: Vec<tokio::task::JoinHandle<()>> = cfg
        .endpoints
        .iter()
        .map(|ep| {
            monitor::spawn_monitor(
                ep.clone(),
                Arc::clone(&global),
                Arc::clone(&shared_state),
                Arc::clone(&shared_log),
                client.clone(),
            )
        })
        .collect();

    // Run the TUI on the current thread (blocks until the user presses q / Ctrl-C).
    // `block_in_place` yields the tokio worker thread to other tasks while we block.
    let tui_result = tokio::task::block_in_place(|| {
        ui::run_ui(&cfg, Arc::clone(&shared_state), Arc::clone(&shared_log))
    });

    // Graceful shutdown: stop all monitor tasks
    for handle in handles {
        handle.abort();
    }

    // Persist state to disk
    let map = shared_state.lock().unwrap();
    if let Err(e) = state::save_state(&state_path, &map) {
        eprintln!("Warning: could not save state: {e}");
    }

    tui_result
}

