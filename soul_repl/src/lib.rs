use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use soul_llm::{LlmConfig, OllamaClient};
use soul_planner::CognitiveLoop;
use soul_tools::{discover_system_tools, execute_shell, ToolRegistry};

pub struct ReplState {
    pub llm: OllamaClient,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        let tools = discover_system_tools();
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }
        Self {
            llm: OllamaClient::new(config),
            planner: CognitiveLoop::new(),
            registry,
        }
    }
}

pub fn run_repl(state: &mut ReplState) {
    let mut rl = DefaultEditor::new().expect("Failed to create readline");

    println!("{}", "SoulSystem Autonomous REPL".cyan().bold());
    println!("{}", "Type 'help' for commands, 'exit' to quit".dimmed());
    println!();

    loop {
        let prompt = format!("{} ", ">>".green().bold());
        match rl.readline(&prompt) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);
                handle_input(state, input);
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "CTRL-C".yellow());
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", "Goodbye!".cyan());
                break;
            }
            Err(e) => {
                println!("{}: {:?}", "Error".red().bold(), e);
                break;
            }
        }
    }
}

fn handle_input(state: &mut ReplState, input: &str) {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let args = parts.get(1).unwrap_or(&"");

    match cmd.as_str() {
        "help" => print_help(),
        "exit" | "quit" => {
            println!("{}", "Goodbye!".cyan());
            std::process::exit(0);
        }
        "ask" => {
            if args.is_empty() {
                println!("{}", "Usage: ask <question>".yellow());
                return;
            }
            let rt = tokio::runtime::Runtime::new().unwrap();
            let resp = rt.block_on(async {
                state.llm.generate(args).await
            });
            match resp {
                Ok(r) => println!("{}", r.response),
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }
        "plan" => {
            if args.is_empty() {
                println!("{}", "Usage: plan <goal description>".yellow());
                return;
            }
            let goal = soul_planner::Goal {
                id: uuid::Uuid::new_v4().to_string(),
                description: args.to_string(),
                priority: 5,
                created_at: chrono::Utc::now(),
                status: soul_planner::GoalStatus::Active,
            };
            let plan = state.planner.create_plan(&goal, &[]);
            println!("{}: {}", "Plan".cyan().bold(), serde_json::to_string_pretty(&plan).unwrap());
        }
        "tools" => {
            let tools = state.registry.list();
            if tools.is_empty() {
                println!("{}", "No tools discovered".yellow());
            } else {
                println!("{}: {} tools available", "Tools".cyan().bold(), tools.len());
                for t in tools.iter().take(20) {
                    println!("  {} - {}", t.name.green(), t.description.dimmed());
                }
                if tools.len() > 20 {
                    println!("  {} more...", format!("...{}", tools.len() - 20).dimmed());
                }
            }
        }
        "run" => {
            if args.is_empty() {
                println!("{}", "Usage: run <shell command>".yellow());
                return;
            }
            match execute_shell(args) {
                Ok(out) => {
                    if out.trim().is_empty() {
                        println!("{}", "(no output)".dimmed());
                    } else {
                        println!("{}", out);
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }
        "memory" => {
            let obs = state.planner.memory.recent_observations(10);
            if obs.is_empty() {
                println!("{}", "Memory empty".dimmed());
            } else {
                println!("{}:", "Recent Observations".cyan().bold());
                for o in obs {
                    println!("  - {}", o);
                }
            }
        }
        "observe" => {
            if args.is_empty() {
                println!("{}", "Usage: observe <observation>".yellow());
                return;
            }
            state.planner.memory.observe(args.to_string());
            println!("{}: {}", "Observed".green().bold(), args);
        }
        "decide" => {
            let decision = state.planner.decide(args);
            println!("{}: {}", "Decision".cyan().bold(), serde_json::to_string_pretty(&decision).unwrap());
        }
        "history" => {
            let recent = state.planner.history.recent(10);
            if recent.is_empty() {
                println!("{}", "No action history".dimmed());
            } else {
                println!("{}:", "Action History".cyan().bold());
                for a in recent {
                    let status = if a.success { "✓".green() } else { "✗".red() };
                    println!("  {} {} → {}", status, a.action, a.result);
                }
            }
        }
        "status" => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let alive = rt.block_on(async { state.llm.is_alive().await });
            let model = &state.llm.config().model;
            let tools = state.registry.list().len();
            let rate = state.planner.history.success_rate();
            println!("{}:", "Status".cyan().bold());
            println!("  Model: {}", model.green());
            println!("  Ollama: {}", if alive { "connected".green() } else { "disconnected".red() });
            println!("  Tools: {}", tools.to_string().green());
            println!("  Success rate: {:.1}%", (rate * 100.0));
        }
        "models" => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            match rt.block_on(async { state.llm.list_models().await }) {
                Ok(models) => {
                    println!("{}: {} models", "Models".cyan().bold(), models.len());
                    for m in models.iter().take(20) {
                        println!("  {} - {}", m.name.green(), m.parameter_size.as_deref().unwrap_or("?").dimmed());
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }
        _ => {
            println!("{}: unknown command '{}'. Type 'help' for commands.", "Error".red(), cmd);
        }
    }
}

fn print_help() {
    println!("{}", "Commands:".cyan().bold());
    println!("  {} - Ask a question to the LLM", "ask <msg>".green());
    println!("  {} - Create a plan for a goal", "plan <goal>".green());
    println!("  {} - Execute a shell command", "run <cmd>".green());
    println!("  {} - List available tools", "tools".green());
    println!("  {} - View working memory", "memory".green());
    println!("  {} - Record an observation", "observe <msg>".green());
    println!("  {} - Make a decision", "decide <ctx>".green());
    println!("  {} - View action history", "history".green());
    println!("  {} - System status", "status".green());
    println!("  {} - List Ollama models", "models".green());
    println!("  {} - Show this help", "help".green());
    println!("  {} - Exit", "exit".green());
}
