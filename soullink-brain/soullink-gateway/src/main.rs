use clap::Parser;

use soullink_gateway::cli::args::{Cli, Commands};
use soullink_gateway::cli::call::{call_cmd, CallOpts};
use soullink_gateway::cli::discover::{discover_cmd, DiscoverOpts};
use soullink_gateway::cli::health::{health_cmd, HealthOpts};
use soullink_gateway::cli::probe::{probe_cmd, ProbeOpts};
use soullink_gateway::cli::run::{run_cmd, RunOpts};
use soullink_gateway::cli::status::{status_cmd, StatusOpts};

#[tokio::main(worker_threads = 4)]
async fn main() -> std::process::ExitCode {
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
                token,
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
