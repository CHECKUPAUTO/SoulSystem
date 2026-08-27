//! Command-line entry point for the canonical SoulSystem coding harness.
//!
//! This binary intentionally exposes one execution path: create a detached
//! Git worktree, run the provider-agnostic coding loop, and print an
//! evidence-bearing result. Provider credentials are read from the existing
//! environment/credential conventions; they are never accepted as a CLI
//! argument that could land in shell history.

use clap::{Parser, ValueEnum};
use soul_coding::{
    CheckSpec, CodingAgent, CodingAgentEvent, GitWorkspace, TaskResult, TaskSpec, TaskStatus,
};
use soul_llm::{LlmClient, LlmConfig, ProviderKind};
use soul_sandbox::SandboxPolicy;
use soullink_gate::ExecutionMode;
use soulsystem_common::secrets::SecretString;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Parser)]
#[command(
    name = "soul-coding",
    about = "Run the canonical SoulSystem coding harness in an isolated Git worktree"
)]
struct Args {
    /// Repository to modify. The worktree is created below this directory.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Revision from which the detached coding worktree is created.
    #[arg(long, default_value = "HEAD")]
    base_revision: String,

    /// Natural-language coding task.
    #[arg(long)]
    prompt: String,

    /// Required shell-free acceptance check, repeated as NAME=COMMAND.
    /// Commands are whitespace-separated argv; shell pipes and separators are
    /// rejected by the verifier.
    #[arg(long = "check", value_name = "NAME=COMMAND", required = true)]
    checks: Vec<String>,

    /// Optional stable session identity. A UUID is generated when omitted.
    #[arg(long)]
    session_id: Option<String>,

    /// LLM provider: ollama, openai, or anthropic.
    #[arg(long, default_value = "ollama")]
    provider: String,

    /// Provider model name. Defaults to the provider config default.
    #[arg(long)]
    model: Option<String>,

    /// Provider base URL. Defaults to the provider config default.
    #[arg(long)]
    base_url: Option<String>,

    /// Approval/sandbox mode. Autonomous blocks critical operations.
    #[arg(long, value_enum, default_value_t = ModeArg::Autonomous)]
    mode: ModeArg,

    /// Print only the final JSON result on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModeArg {
    Interactive,
    Autonomous,
    Container,
}

impl ModeArg {
    fn execution_mode(self) -> ExecutionMode {
        match self {
            Self::Interactive => ExecutionMode::Interactive,
            Self::Autonomous => ExecutionMode::Autonomous,
            Self::Container => ExecutionMode::Container,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let provider: ProviderKind = args.provider.parse()?;
    let checks = args
        .checks
        .iter()
        .map(|raw| parse_check(raw))
        .collect::<Result<Vec<_>, _>>()?;
    let task = TaskSpec::new(args.prompt, checks)?;

    let session_id = args
        .session_id
        .unwrap_or_else(|| format!("session-{}", uuid::Uuid::new_v4()));
    let workspace = GitWorkspace::create(
        &args.repo,
        args.base_revision,
        session_id,
        SandboxPolicy::default(),
    )?;

    let mut config = LlmConfig {
        provider,
        ..LlmConfig::default()
    };
    if let Some(model) = args.model {
        config.model = model;
    }
    if let Some(base_url) = args.base_url {
        config.base_url = base_url;
    }
    config.auth_token = provider_token(provider);
    let client = LlmClient::new(config)?;
    let mut agent = CodingAgent::new(
        client,
        Default::default(),
        SandboxPolicy::default(),
        args.mode.execution_mode(),
    );

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    agent.set_event_sender(event_tx);
    let json_output = args.json;
    let event_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if !json_output {
                print_event(event);
            }
        }
    });

    let result = agent.run(&task, &workspace).await;
    drop(agent);
    let _ = event_task.await;
    let result = result?;
    print_result(&result, &workspace, args.json)?;

    if result.status != TaskStatus::Completed {
        std::process::exit(2);
    }
    Ok(())
}

fn parse_check(raw: &str) -> Result<CheckSpec, String> {
    let (name, command) = raw
        .split_once('=')
        .ok_or_else(|| format!("check must use NAME=COMMAND syntax: {raw:?}"))?;
    CheckSpec::required(name, command, 300).map_err(|error| error.to_string())
}

fn provider_token(provider: ProviderKind) -> Option<SecretString> {
    let variable = match provider {
        ProviderKind::Ollama => "SOULSYSTEM_LLM_API_KEY",
        ProviderKind::OpenAI => "OPENAI_API_KEY",
        ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
    };
    std::env::var(variable).ok().map(SecretString::new)
}

fn print_event(event: CodingAgentEvent) {
    match event {
        CodingAgentEvent::TurnStarted { turn } => eprintln!("turn {turn}"),
        CodingAgentEvent::ModelResponse { content } => {
            if !content.trim().is_empty() {
                eprintln!("model: {content}");
            }
        }
        CodingAgentEvent::ToolCall { name } => eprintln!("tool: {name}"),
        CodingAgentEvent::ToolResult { name, success } => {
            eprintln!(
                "tool result: {name} ({})",
                if success { "ok" } else { "failed" }
            );
        }
        CodingAgentEvent::Verification { status } => eprintln!("verification: {status:?}"),
    }
}

fn print_result(
    result: &TaskResult,
    workspace: &GitWorkspace<soul_coding::SandboxCommandRunner>,
    json: bool,
) -> Result<(), serde_json::Error> {
    if json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    println!("status: {:?}", result.status);
    println!("summary: {}", result.summary);
    println!("worktree: {}", workspace.context().worktree().display());
    if let Some(reason) = &result.failure_reason {
        println!("reason: {reason}");
    }
    println!(
        "checks: {}/{} passed",
        result.checks.iter().filter(|check| check.passed).count(),
        result.checks.len()
    );
    Ok(())
}
