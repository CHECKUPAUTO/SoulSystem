use clap::Parser;

use soullink_gateway::cli::args::{Cli, Commands};
use soullink_gateway::cli::call::{call_cmd, CallOpts};
use soullink_gateway::cli::discover::{discover_cmd, DiscoverOpts};
use soullink_gateway::cli::health::{health_cmd, HealthOpts};
use soullink_gateway::cli::probe::{probe_cmd, ProbeOpts};
use soullink_gateway::cli::run::{run_cmd, RunOpts};
use soullink_gateway::cli::status::{status_cmd, StatusOpts};

fn main() -> std::process::ExitCode {
    // OpenClaw-compatible: honor the `WORKERS` env (clamped 1..=64) for the
    // tokio runtime instead of a compile-time fixed thread count.
    let workers = std::env::var("WORKERS")
        .ok()
        .and_then(|w| w.parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 64);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to build tokio runtime: {e}");
            return std::process::ExitCode::from(1);
        }
    };
    runtime.block_on(async_main())
}

async fn async_main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            port,
            bind,
            token,
            auth,
            password,
            password_file,
            config,
            orchestrator_url,
            verbose,
            json,
        } => {
            run_cmd(RunOpts {
                port,
                bind,
                token,
                auth,
                password,
                password_file,
                config_path: config,
                orchestrator_url,
                verbose,
                json,
            })
            .await
        }
        Commands::Status { probe, json } => status_cmd(StatusOpts { probe, json }).await,
        Commands::Call {
            method,
            params,
            url,
            token,
            timeout,
            json,
        } => {
            call_cmd(CallOpts {
                method,
                params,
                url,
                // Wrapped at the CLI boundary so the token is typed for the
                // whole of its life inside the process (INV-SEC-2).
                token: token.map(soulsystem_common::secrets::SecretString::new),
                timeout,
                json,
            })
            .await
        }
        Commands::Health { json } => health_cmd(HealthOpts { json }).await,
        Commands::Discover { timeout, json } => discover_cmd(DiscoverOpts { timeout, json }).await,
        Commands::Probe { url, json } => probe_cmd(ProbeOpts { url, json }).await,
    }
}
