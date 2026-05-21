mod alert;
mod config;
mod monitor;
mod state;
mod ui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "tortuga", about = "HTTP endpoint monitor")]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = config::load(&args.config)?;

    println!("tortuga — loaded {} endpoint(s):", cfg.endpoints.len());
    for ep in &cfg.endpoints {
        println!(
            "  [{:>3}s]  {}  →  {}",
            ep.interval_secs, ep.name, ep.url
        );
    }
    println!("\n(Full runtime not yet implemented — phases 2-6 pending)");
    Ok(())
}

