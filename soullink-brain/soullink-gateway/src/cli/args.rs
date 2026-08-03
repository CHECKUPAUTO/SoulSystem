//! Clap CLI definition — all subcommands mirroring SoulSystem gateway-cli.
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "soullink-gateway")]
#[command(version = crate::cli::banner::VERSION)]
#[command(about = "SoulLink Gateway RS — Messaging bridge to the orchestrator")]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the gateway (foreground)
    Run {
        /// Port for the gateway health/metrics endpoint
        #[arg(short, long, default_value = "9092")]
        port: u16,

        /// Bind mode (loopback|lan|auto)
        #[arg(short, long, default_value = "loopback")]
        bind: String,

        /// Auth token for gateway connections
        #[arg(long)]
        token: Option<String>,

        /// Auth mode (none|token|password)
        #[arg(long)]
        auth: Option<String>,

        /// Password for auth mode=password
        #[arg(long)]
        password: Option<String>,

        /// Read password from file
        #[arg(long)]
        password_file: Option<String>,

        /// Path to config file
        #[arg(short, long)]
        config: Option<String>,

        /// Orchestrator URL override
        #[arg(long)]
        orchestrator_url: Option<String>,

        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,

        /// Output JSON
        #[arg(long)]
        json: bool,
    },

    /// Show gateway service status
    Status {
        /// Probe the gateway for reachability
        #[arg(long)]
        probe: bool,

        /// Output JSON
        #[arg(long)]
        json: bool,
    },

    /// Call a gateway RPC method
    Call {
        /// Method name (health/status)
        method: String,

        /// JSON object string for params
        #[arg(long, default_value = "{}")]
        params: String,

        /// Gateway URL
        #[arg(long)]
        url: Option<String>,

        /// Auth token
        #[arg(long)]
        token: Option<String>,

        /// Timeout in ms
        #[arg(long, default_value = "10000")]
        timeout: u64,

        /// Output JSON
        #[arg(long)]
        json: bool,
    },

    /// Fetch gateway health
    Health {
        /// Output JSON
        #[arg(long)]
        json: bool,
    },

    /// Discover gateways via mDNS/Bonjour
    Discover {
        /// Discovery timeout in ms
        #[arg(long, default_value = "2000")]
        timeout: u64,

        /// Output JSON
        #[arg(long)]
        json: bool,
    },

    /// Probe gateway reachability + status summary
    Probe {
        /// Gateway URL
        #[arg(long)]
        url: Option<String>,

        /// Output JSON
        #[arg(long)]
        json: bool,
    },
}
