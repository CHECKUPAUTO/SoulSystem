use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use soul_llm::{LlmConfig, OllamaClientBlocking};
use soul_planner::CognitiveLoop;
use soul_tools::{discover_system_tools, execute_shell, ToolRegistry};
use soul_automation::{AlertOperator, AlertSeverity};
use soul_security::SecurityEngine;
use soul_monitor::MonitorEngine;
use soul_cognitive::CognitiveEngine;

pub struct ReplState {
    pub llm: OllamaClientBlocking,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
    pub cognitive: CognitiveEngine,
    pub security: SecurityEngine,
    pub monitor: MonitorEngine,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        let tools = discover_system_tools();
        let mut registry = ToolRegistry::new();
        for tool in tools {
            registry.register(tool);
        }
        Self {
            llm: OllamaClientBlocking::new(config),
            planner: CognitiveLoop::new(),
            registry,
            cognitive: CognitiveEngine::new(),
            security: SecurityEngine::new(),
            monitor: MonitorEngine::new(),
        }
    }
}

pub fn run_repl(state: &mut ReplState) {
    let mut rl = DefaultEditor::new().unwrap_or_else(|_| {
        eprintln!("Failed to create readline editor");
        std::process::exit(1);
    });

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
            match state.llm.generate(args) {
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
            let tool_names: Vec<String> = state.registry.list().iter().map(|t| t.name.clone()).collect();
            println!("{}", "Planning with LLM...".dimmed());
            let plan = state.planner.create_plan(&goal, &tool_names);
            match serde_json::to_string_pretty(&plan) {
                Ok(json) => println!("{}: {}", "Plan".cyan().bold(), json),
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
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
            state.cognitive.context.add(args, 0.5);
            println!("{}: {}", "Observed".green().bold(), args);
        }
        "decide" => {
            println!("{}", "Thinking...".dimmed());
            let decision = state.planner.decide(args);
            match serde_json::to_string_pretty(&decision) {
                Ok(json) => println!("{}: {}", "Decision".cyan().bold(), json),
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
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
        "learn" => {
            let learn_parts: Vec<&str> = args.splitn(3, ' ').collect();
            if learn_parts.len() < 3 {
                println!("{}", "Usage: learn <action> <outcome> <reward:0.0-1.0>".yellow());
                return;
            }
            let reward = learn_parts[2].parse::<f32>().unwrap_or(0.5);
            state.cognitive.learn(learn_parts[0], learn_parts[1], reward);
            state.planner.history.record(
                learn_parts[0].to_string(),
                learn_parts[1].to_string(),
                reward > 0.0,
            );
            println!("{}: learned '{}' → '{}' (reward: {:.1})", "Learned".green().bold(), learn_parts[0], learn_parts[1], reward);
        }
        "think" => {
            if args.is_empty() {
                println!("{}", "Usage: think <input>".yellow());
                return;
            }
            let result = state.cognitive.think(args);
            match serde_json::to_string_pretty(&result) {
                Ok(json) => println!("{}: {}", "Think".cyan().bold(), json),
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }
        "knowledge" => {
            let stats = state.cognitive.knowledge.stats();
            println!("{}: {}", "Knowledge Graph".cyan().bold(), stats);
            let entities = state.cognitive.knowledge.search(args);
            if !entities.is_empty() {
                println!("  Found {} entities:", entities.len());
                for e in entities.iter().take(10) {
                    println!("    {} ({})", e.name.green(), e.entity_type.dimmed());
                }
            }
        }
        "graph" => {
            if args.is_empty() {
                let stats = state.cognitive.knowledge.stats();
                println!("{}: {}", "Knowledge Graph".cyan().bold(), stats);
            } else {
                let results = state.cognitive.knowledge.search(args);
                if results.is_empty() {
                    println!("{}", "No matching entities".dimmed());
                } else {
                    for e in results {
                        println!("  {} ({})", e.name.green(), e.entity_type.dimmed());
                        let related = state.cognitive.knowledge.get_related(&e.id);
                        for r in related.iter().take(5) {
                            println!("    → {} ({})", r.name, r.entity_type);
                        }
                    }
                }
            }
        }
        "clear" => {
            state.planner.memory.observations.clear();
            state.planner.history.actions.clear();
            println!("{}", "Memory and history cleared".green().bold());
        }
        "config" => {
            let cfg = state.llm.config();
            println!("{}:", "Configuration".cyan().bold());
            println!("  Model: {}", cfg.model.green());
            println!("  URL: {}", cfg.base_url);
            println!("  Temperature: {}", cfg.temperature);
            println!("  Max tokens: {}", cfg.max_tokens);
        }
        "gpu" => {
            match state.monitor.gpu.get_gpu_info() {
                Some(gpu) => {
                    println!("{}:", "GPU".cyan().bold());
                    println!("  Name: {}", gpu.name.green());
                    println!("  Temperature: {:.0}°C", gpu.temperature);
                    println!("  Utilization: {:.1}%", gpu.utilization);
                    println!("  Memory: {} MB / {} MB", gpu.memory_used, gpu.memory_total);
                    println!("  Power: {:.0}W", gpu.power);
                }
                None => println!("{}", "No GPU detected".yellow()),
            }
        }
        "network" => {
            let stats = state.monitor.network.get_stats();
            println!("{}:", "Network".cyan().bold());
            println!("  Connections: {}", stats.connections);
            println!("  Latency: {:.1}ms", stats.latency_ms);
            for iface in &stats.interfaces {
                println!("  {}: ↑{} ↓{}", iface.name.green(),
                    format_bytes(iface.bytes_sent),
                    format_bytes(iface.bytes_received));
            }
        }
        "disks" => {
            let disks = state.monitor.disk.get_disks();
            println!("{}:", "Disks".cyan().bold());
            for d in &disks {
                println!("  {} → {} ({:.1}% used)",
                    d.device.green(), d.mount_point, d.usage_percent);
            }
        }
        "audit" => {
            let recent = state.security.audit.recent(10);
            if recent.is_empty() {
                println!("{}", "No audit entries".dimmed());
            } else {
                println!("{}:", "Audit Trail".cyan().bold());
                for e in recent {
                    println!("  [{}] {} → {}: {}",
                        e.timestamp.format("%H:%M"),
                        e.actor.green(),
                        e.target,
                        e.action);
                }
            }
        }
        "secrets" => {
            let secrets = state.security.secrets.list_secrets();
            println!("{}: {} secrets", "Secrets".cyan().bold(), secrets.len());
            for s in secrets {
                println!("  {} (created: {})", s.name.green(),
                    s.created_at.format("%Y-%m-%d"));
            }
        }
        "block" => {
            if args.is_empty() {
                println!("{}", "Usage: block <ip>".yellow());
                return;
            }
            state.security.intrusion.block_ip(args);
            println!("{}: {}", "Blocked".red().bold(), args);
        }
        "intrusions" => {
            let events = state.security.intrusion.recent_events(10);
            if events.is_empty() {
                println!("{}", "No intrusion events".dimmed());
            } else {
                println!("{}:", "Intrusion Events".cyan().bold());
                for e in events {
                    println!("  [{}] {:?} from {}: {}",
                        e.timestamp.format("%H:%M"),
                        e.severity,
                        e.source_ip.green(),
                        e.details);
                }
            }
        }
        "rate" => {
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.len() >= 2 {
                let max: usize = parts[1].parse().unwrap_or(100);
                state.security.rate_limiter.set_limit(parts[0], max, 60);
                println!("{}: {} → max {}/min", "Rate limit".green().bold(), parts[0], max);
            } else if !args.is_empty() {
                let remaining = state.security.rate_limiter.remaining(args);
                println!("{}: {} remaining", "Rate limit".cyan().bold(), remaining);
            } else {
                println!("{}", "Usage: rate <key> [max_requests]".yellow());
            }
        }
        "status" => {
            print_full_status(state);
        }
        "models" => {
            match state.llm.list_models() {
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

fn print_full_status(state: &ReplState) {
    let alive = state.llm.is_alive();
    let model = &state.llm.config().model;
    let tools = state.registry.list().len();
    let rate = state.planner.history.success_rate();
    let metrics = soul_bridges::monitor::get_metrics();
    let docker_running = soul_bridges::docker::is_docker_running();
    let containers = soul_bridges::docker::list_containers().unwrap_or_default();

    println!("{}:", "System Status".cyan().bold());
    println!("  LLM:");
    println!("    Model: {}", model.green());
    println!("    Ollama: {}", if alive { "connected".green() } else { "disconnected".red() });
    println!("  Resources:");
    println!("    CPU: {:.1}%", metrics.cpu_usage);
    println!("    Memory: {:.1}% ({} MB / {} MB)",
        metrics.memory_usage,
        (metrics.memory_total - metrics.memory_available) / 1024,
        metrics.memory_total / 1024);
    println!("    Processes: {}", metrics.process_count);
    println!("    Load: {:.2} / {:.2} / {:.2}", metrics.load_avg[0], metrics.load_avg[1], metrics.load_avg[2]);
    println!("  Services:");
    println!("    Tools: {}", tools.to_string().green());
    println!("    Docker: {}", if docker_running { "running".green() } else { "stopped".red() });
    if !containers.is_empty() {
        println!("    Containers: {}", containers.len());
        for c in containers.iter().take(5) {
            println!("      {} - {}", c.name.green(), c.status);
        }
    }
    println!("  Cognitive:");
    println!("    Knowledge: {} entities", state.cognitive.knowledge.stats()["entities"]);
    println!("    Learning rate: {:.1}%", state.cognitive.learning.success_rate() * 100.0);
    println!("    Context: {}", state.cognitive.context.stats());
    println!("  Security:");
    println!("    Audit entries: {}", state.security.audit.count());
    println!("    Intrusion events: {}", state.security.intrusion.stats()["total_events"]);
    println!("    Secrets: {}", state.security.secrets.list_secrets().len());
    println!("  Performance:");
    println!("    Success rate: {:.1}%", (rate * 100.0));
    println!("    Memory entries: {}", state.planner.memory.observations.len());
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn print_help() {
    println!("{}", "Commands:".cyan().bold());
    println!("  {} - Ask a question to the LLM", "ask <msg>".green());
    println!("  {} - Create a plan for a goal (LLM-powered)", "plan <goal>".green());
    println!("  {} - Execute a shell command", "run <cmd>".green());
    println!("  {} - List available tools", "tools".green());
    println!("  {} - View working memory", "memory".green());
    println!("  {} - Record an observation", "observe <msg>".green());
    println!("  {} - Make a decision (LLM-powered)", "decide <ctx>".green());
    println!("  {} - View action history", "history".green());
    println!("  {} - Learn from experience", "learn <action> <outcome> <reward>".green());
    println!("  {} - Think about input (cognitive)", "think <input>".green());
    println!("  {} - Query knowledge graph", "knowledge [query]".green());
    println!("  {} - Explore knowledge graph", "graph [query]".green());
    println!("  {} - Clear memory and history", "clear".green());
    println!("  {} - Show configuration", "config".green());
    println!("  {} - GPU status", "gpu".green());
    println!("  {} - Network status", "network".green());
    println!("  {} - Disk status", "disks".green());
    println!("  {} - View audit trail", "audit".green());
    println!("  {} - List secrets", "secrets".green());
    println!("  {} - Block an IP", "block <ip>".green());
    println!("  {} - View intrusion events", "intrusions".green());
    println!("  {} - Rate limit management", "rate <key> [max]".green());
    println!("  {} - Full system status", "status".green());
    println!("  {} - List Ollama models", "models".green());
    println!("  {} - Show this help", "help".green());
    println!("  {} - Exit", "exit".green());
}
