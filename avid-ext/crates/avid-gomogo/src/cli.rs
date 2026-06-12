use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gomogo")]
#[command(about = "API Reverse Engineering & Captcha Farm", version)]
pub struct Cli {
    /// Fichier de configuration TOML
    #[arg(short = 'C', long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Subcommand)]
pub enum Mode {
    /// Découverte de endpoints et inspection des réponses
    Scanner(ScannerArgs),
    /// Proxy MITM pour capturer le trafic (mobile, web)
    Proxy(ProxyArgs),
    /// Affiche une liste exhaustive des techniques de reverse engineering d'API
    Techniques,
    /// Lance le serveur de ferme à CAPTCHAs
    Captcha(CaptchaArgs),
    /// Génère un fichier de configuration par défaut
    Init(InitArgs),
}

#[derive(clap::Args)]
pub struct ScannerArgs {
    #[arg(short, long)]
    pub base_url: String,
    #[arg(short, long)]
    pub wordlist: String,
    #[arg(short = 'X', long, default_value = "GET")]
    pub method: String,
    #[arg(long)]
    pub data: Option<String>,
    #[arg(short = 'H', long = "header")]
    pub headers: Vec<String>,
    #[arg(short = 'c', long)]
    pub concurrency: Option<usize>,
    #[arg(short = 'd', long)]
    pub delay_ms: Option<u64>,
    #[arg(short = 't', long)]
    pub timeout_secs: Option<u64>,
    #[arg(long)]
    pub follow_redirects: Option<bool>,
    #[arg(long)]
    pub require_json: bool,
    #[arg(long)]
    pub contains: Option<String>,
    #[arg(long)]
    pub regex_match: Option<String>,
    #[arg(long)]
    pub json_output: Option<PathBuf>,
    #[arg(long)]
    pub show_response_time: bool,
    #[arg(long)]
    pub rotate_user_agent: bool,
    #[arg(long)]
    pub proxy: Option<String>,
    #[arg(long)]
    pub http2: bool,
    #[arg(long)]
    pub resume: Option<PathBuf>,
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(clap::Args)]
pub struct ProxyArgs {
    #[arg(short, long)]
    pub listen: Option<String>,
    #[arg(short, long)]
    pub cert_dir: Option<PathBuf>,
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub dashboard: bool,
    #[arg(long, default_value_t = 9090)]
    pub dashboard_port: u16,
    #[arg(long)]
    pub whitelist: Vec<String>,
    #[arg(long)]
    pub blacklist: Vec<String>,
    #[arg(long)]
    pub har_output: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct CaptchaArgs {
    #[arg(short, long)]
    pub port: Option<u16>,
    #[arg(short, long)]
    pub bind: Option<String>,
    #[arg(long)]
    pub task_timeout_secs: Option<u64>,
    #[arg(long)]
    pub result_ttl_secs: Option<u64>,
}

#[derive(clap::Args)]
pub struct InitArgs {
    #[arg(short, long, default_value = "config.toml")]
    pub output: PathBuf,
}
