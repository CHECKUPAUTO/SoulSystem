use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use soul_agent_core::{AgentConfig, AutonomousAgent};
use soul_cognitive::CognitiveEngine;
use soul_conversations::ConversationStore;
use soul_critique::quick_critique;
use soul_designtree::{DesignState, DesignTree};
use soul_graph_memory::{Edge, EdgeType, KnowledgeGraph, Node, NodeType};
use soul_inference::InferenceController;
use soul_llm::{LlmConfig, OllamaClient, OllamaClientBlocking};
use soul_mcp::McpToolHandler;
use soul_monitor::MonitorEngine;
use soul_persist::PersistentStore;
use soul_planner::CognitiveLoop;
use soul_security::SecurityEngine;
use soul_skills::SkillLoader;
use soul_subagents::SubAgentManager;
use soul_tools::{discover_system_tools, execute_shell, ToolRegistry};
use tokio::sync::broadcast;

pub struct ReplState {
    pub llm: OllamaClientBlocking,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
    pub agent: AutonomousAgent,
    pub cognitive: CognitiveEngine,
    pub security: SecurityEngine,
    pub monitor: MonitorEngine,
    pub design_tree: DesignTree,
    pub graph: KnowledgeGraph,
    pub persist_store: Option<PersistentStore>,
    pub sub_agents: SubAgentManager,
    pub skill_loader: SkillLoader,
    pub conversations: Option<ConversationStore>,
    pub session_id: Option<String>,
    pub verbose: bool,
    pub daemon_rx: Option<broadcast::Receiver<soul_dashboard::BusEvent>>,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        // Synchronous REPL tool registry (system tool discovery).
        let mut registry = ToolRegistry::new();
        for tool in discover_system_tools() {
            registry.register(tool);
        }

        // Data directories for persistence, design tree, and skills.
        let data_dir = std::env::temp_dir().join("soul_repl_data");
        std::fs::create_dir_all(&data_dir).ok();
        let design_dir = data_dir.join("design");
        std::fs::create_dir_all(&design_dir).ok();
        let skills_dir = data_dir.join("skills");
        std::fs::create_dir_all(&skills_dir).ok();
        let persist_dir = data_dir.join("persist");
        std::fs::create_dir_all(&persist_dir).ok();

        // Optional stores degrade gracefully when their backends are unavailable.
        let persist_store = PersistentStore::open(&persist_dir).ok();
        let conversations = ConversationStore::open(&data_dir.join("conversations.db")).ok();

        let agent =
            AutonomousAgent::new(OllamaClient::new(config.clone()), AgentConfig::default());

        Self {
            llm: OllamaClientBlocking::new(config.clone()),
            planner: CognitiveLoop::new(),
            registry,
            agent,
            cognitive: CognitiveEngine::new(),
            security: SecurityEngine::new(),
            monitor: MonitorEngine::new(),
            design_tree: DesignTree::new(&design_dir),
            graph: KnowledgeGraph::new(),
            persist_store,
            sub_agents: SubAgentManager::new(config, 4),
            skill_loader: SkillLoader::new(&skills_dir),
            conversations,
            session_id: None,
            verbose: false,
            daemon_rx: None,
        }
    }

    pub fn with_daemon_events(mut self, rx: broadcast::Receiver<soul_dashboard::BusEvent>) -> Self {
        self.daemon_rx = Some(rx);
        self
    }
}

fn new_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap_or_else(|e| {
        eprintln!("Failed to create tokio runtime: {e}");
        std::process::exit(1);
    })
}

pub fn run_repl(state: &mut ReplState) {
    let mut rl = DefaultEditor::new().unwrap_or_else(|_| {
        eprintln!("Failed to create readline editor");
        std::process::exit(1);
    });

    println!("{}", "╔══════════════════════════════════════════╗".cyan());
    println!("{}", "║   SoulSystem Autonomous Agent v0.2.0   ║".cyan().bold());
    println!("{}", "║   Type 'help' for commands              ║".cyan());
    println!("{}", "╚══════════════════════════════════════════╝".cyan());
    println!();

    let rt = new_runtime();
    loop {
        let prompt = format!("{} ", ">>".green().bold());
        match rl.readline(&prompt) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(input);
                rt.block_on(handle_input(state, input));
            }
            Err(ReadlineError::Interrupted) => {
                println!("{}", "CTRL-C — use 'exit' to quit".yellow());
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

async fn handle_input(state: &mut ReplState, input: &str) {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let args = parts.get(1).unwrap_or(&"");

    match cmd.as_str() {
        "help" => print_help(),
        "exit" | "quit" => {
            println!("{}", "Goodbye!".cyan());
            std::process::exit(0);
        }

        // ── Ask with conversation context ──
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

        // ── Plan a goal ──
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

        // ── Tools ──
        "tools" => {
            let tools = state.agent.registry.list();
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

        // ── Direct shell execution ──
        "shell" | "exec" => {
            if args.is_empty() {
                println!("{}", "Usage: shell <command>".yellow());
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

        // ── Memory ──
        "memory" => {
            let obs = state.planner.memory.recent_observations(10);
            let key_info = &state.planner.memory.key_info;
            println!("{}:", "Memory".cyan().bold());
            if !key_info.is_empty() {
                println!("  Key Info: {}", key_info.green());
            }
            if obs.is_empty() {
                println!("  {}", "(no observations)".dimmed());
            } else {
                println!("  Recent Observations:");
                for o in obs {
                    println!("    - {}", o);
                }
            }
            println!("  History: {} actions", state.planner.history.actions.len());
            println!("  Success Rate: {:.1}%", state.planner.history.success_rate() * 100.0);
        }

        // ── Observe ──
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
            let recent = state.planner.history.recent(15);
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

        // ── Models ──
        "models" => {
            match state.llm.list_models() {
                Ok(models) => {
                    println!("{}: {} models", "Models".cyan().bold(), models.len());
                    for m in models.iter().take(20) {
                        println!(
                            "  {} - {}",
                            m.name.green(),
                            m.parameter_size.as_deref().unwrap_or("?").dimmed()
                        );
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }

        // ── Browse web ──
        "browse" | "web" => {
            if args.is_empty() {
                println!("{}", "Usage: browse <url>".yellow());
                return;
            }
            println!("{}", format!("Fetching {}...", args).cyan());
            let fetcher = match soul_webfetch::WebFetcher::new(soul_webfetch::FetcherConfig::default()) {
                Ok(f) => f,
                Err(e) => {
                    println!("Failed to create fetcher: {e}");
                    return;
                }
            };
            match fetcher.fetch(args).await {
                Ok(content) => {
                    println!("{}: {}", "Title".cyan().bold(), content.title.green());
                    println!("{}: {} bytes", "Size".cyan().bold(), content.size_bytes);
                    println!();
                    // Show first 1000 chars of text
                    let preview = if content.text.len() > 1000 {
                        format!("{}...", &content.text[..1000])
                    } else {
                        content.text.clone()
                    };
                    println!("{}", preview);
                    if !content.links.is_empty() {
                        println!();
                        println!("{}: {} links", "Links".cyan().bold(), content.links.len());
                        for link in content.links.iter().take(5) {
                            println!("  {}", link.dimmed());
                        }
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }

        // ── Search and fetch multiple URLs ──
        "fetch" => {
            if args.is_empty() {
                println!("{}", "Usage: fetch <url1> <url2> ...".yellow());
                return;
            }
            let urls: Vec<&str> = args.split_whitespace().collect();
            let fetcher = match soul_webfetch::WebFetcher::new(soul_webfetch::FetcherConfig::default()) {
                Ok(f) => f,
                Err(e) => {
                    println!("Failed to create fetcher: {e}");
                    return;
                }
            };
            let results = fetcher.fetch_many(&urls).await;
            for (i, result) in results.iter().enumerate() {
                match result {
                    Ok(content) => {
                        println!("{}: {} ({})", 
                            format!("[{}]", i + 1).green(),
                            content.title.green(),
                            content.url.dimmed()
                        );
                        let preview = if content.text.len() > 200 {
                            format!("{}...", &content.text[..200])
                        } else {
                            content.text.clone()
                        };
                        println!("  {}", preview.dimmed());
                    }
                    Err(e) => println!("{}: {} - {}", 
                        format!("[{}]", i + 1).red(),
                        urls[i].red(),
                        e
                    ),
                }
            }
        }

        // ── Web search with RAG ──
        "websearch" | "rag" => {
            if args.is_empty() {
                println!("{}", "Usage: websearch <query>".yellow());
                return;
            }
            println!("{}", format!("Searching: {}...", args).cyan());
            let mut rag = soul_rag::RagStore::new(soul_rag::RagConfig::default());
            match rag.search_web(args).await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("{}", "No results found".yellow());
                    } else {
                        println!("{}: {} results", "Results".cyan().bold(), results.len());
                        for (i, content) in results.iter().enumerate() {
                            println!();
                            println!("{} {} {}", 
                                format!("[{}]", i + 1).green().bold(),
                                content.title.green(),
                                format!("(score: {:.1})", content.relevance_score).dimmed()
                            );
                            println!("  {}", content.url.dimmed());
                            let preview = if content.text.len() > 300 {
                                format!("{}...", &content.text[..300])
                            } else {
                                content.text.clone()
                            };
                            println!("  {}", preview);
                        }
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }

        // ── Ask with web context ──
        "askweb" => {
            if args.is_empty() {
                println!("{}", "Usage: askweb <question>".yellow());
                return;
            }
            // Fetch relevant context from web
            println!("{}", "Fetching web context...".cyan());
            let mut rag = soul_rag::RagStore::new(soul_rag::RagConfig::default());
            let context = match rag.search_web(args).await {
                Ok(results) => {
                    results.iter()
                        .map(|r| format!("{}: {}", r.title, r.text.chars().take(500).collect::<String>()))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                }
                Err(_) => String::new(),
            };

            // Build prompt with web context
            let prompt = if context.is_empty() {
                args.to_string()
            } else {
                format!("Based on this web context:\n{}\n\nQuestion: {}", context, args)
            };

            match state.agent.ask(&prompt).await {
                Ok(resp) => println!("\n{}", resp),
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }

        // ── Save context to persistence ──
        "save" => {
            let data_dir = std::env::temp_dir().join("soul_repl_data");
            std::fs::create_dir_all(&data_dir).ok();

            // Save graph
            let graph_path = data_dir.join("graph.json");
            if let Err(e) = state.graph.persist(&graph_path) {
                println!("{}: graph save failed: {}", "Error".red(), e);
            } else {
                let stats = state.graph.stats();
                println!(
                    "{}: graph saved ({} nodes, {} edges)",
                    "✓".green(),
                    stats.node_count,
                    stats.edge_count
                );
            }

            // Save design tree
            let design_dir = data_dir.join("design");
            std::fs::create_dir_all(&design_dir).ok();
            if let Err(e) = state.design_tree.save().await {
                println!("{}: design save failed: {}", "Error".red(), e);
            } else {
                let stats = state.design_tree.stats();
                println!(
                    "{}: design tree saved ({} nodes)",
                    "✓".green(),
                    stats.total
                );
            }

            // Save agent context
            if let Some(ref store) = state.persist_store {
                let mem = soul_persist::PersistentWorkingMemory {
                    key_info: state.planner.memory.key_info.clone(),
                    observations: state.planner.memory.recent_observations(50).to_vec(),
                    related_sop: None,
                    context: serde_json::json!({
                        "turn": state.agent.turn,
                        "tools": state.agent.registry.list().len(),
                    }),
                    last_updated: chrono::Utc::now(),
                };
                store.save_memory(&mem).ok();
                for action in state.planner.history.recent(20) {
                    let entry = soul_persist::ChatEntry {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: "system".into(),
                        content: format!("Action: {} → {}", action.action, action.result),
                        timestamp: chrono::Utc::now(),
                        tool_calls: None,
                        tool_call_id: None,
                    };
                    store.save_chat_entry(&entry).ok();
                }
                println!("{}: agent context saved", "✓".green());
            }
        }

        // ── Load context from persistence ──
        "load" => {
            let data_dir = std::env::temp_dir().join("soul_repl_data");

            // Load graph
            let graph_path = data_dir.join("graph.json");
            if graph_path.exists() {
                match soul_graph_memory::KnowledgeGraph::load(&graph_path) {
                    Ok(loaded) => {
                        state.graph = loaded;
                        let stats = state.graph.stats();
                        println!(
                            "{}: graph loaded ({} nodes, {} edges)",
                            "✓".green(),
                            stats.node_count,
                            stats.edge_count
                        );
                    }
                    Err(e) => println!("{}: graph load failed: {}", "Error".red(), e),
                }
            }

            // Load design tree
            let design_dir = data_dir.join("design");
            if design_dir.exists() {
                let count = state.design_tree.load().await.unwrap_or(0);
                println!("{}: design tree loaded ({} nodes)", "✓".green(), count);
            }

            // Load agent context
            if let Some(ref store) = state.persist_store {
                match store.load_memory() {
                    Ok(mem) => {
                        state.planner.memory.key_info = mem.key_info;
                        for obs in mem.observations {
                            state.planner.memory.observe(obs);
                        }
                        println!("{}: agent context loaded", "✓".green());
                    }
                    Err(_) => println!("{}", "No saved agent context found".yellow()),
                }
            }
        }

        // ── Verbose toggle ──
        "verbose" => {
            state.verbose = !state.verbose;
            println!(
                "Verbose: {}",
                if state.verbose { "ON".green() } else { "OFF".red() }
            );
        }

        // ── Goal management ──
        "goal" => {
            let sub_parts: Vec<&str> = args.splitn(2, ' ').collect();
            let sub_cmd = sub_parts.first().unwrap_or(&"");
            let sub_args = sub_parts.get(1).unwrap_or(&"");

            match *sub_cmd {
                "add" => {
                    if sub_args.is_empty() {
                        println!("{}", "Usage: goal add <description>".yellow());
                        return;
                    }
                    if let Some(ref store) = state.persist_store {
                        let goal = soul_persist::PersistentGoal {
                            id: uuid::Uuid::new_v4().to_string(),
                            description: sub_args.to_string(),
                            priority: 5,
                            status: soul_persist::GoalStatus::Active,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                            parent_id: None,
                            children_ids: vec![],
                            result: None,
                        };
                        match store.save_goal(&goal) {
                            Ok(()) => println!("{}: {} ({})", "Goal added".green().bold(), goal.description, &goal.id[..8]),
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    } else {
                        println!("{}", "Persistence not available".red());
                    }
                }
                "list" => {
                    if let Some(ref store) = state.persist_store {
                        match store.load_all_goals() {
                            Ok(goals) => {
                                if goals.is_empty() {
                                    println!("{}", "No goals found".dimmed());
                                } else {
                                    println!("{}:", "Goals".cyan().bold());
                                    for g in &goals {
                                        let status_icon = match g.status {
                                            soul_persist::GoalStatus::Active => "○".yellow(),
                                            soul_persist::GoalStatus::Running => "●".green(),
                                            soul_persist::GoalStatus::Completed => "✓".cyan(),
                                            soul_persist::GoalStatus::Failed => "✗".red(),
                                            soul_persist::GoalStatus::Deferred => "◦".dimmed(),
                                        };
                                        println!(
                                            "  {} [{}] {} (priority: {})",
                                            status_icon,
                                            &g.id[..8],
                                            g.description,
                                            g.priority
                                        );
                                    }
                                }
                            }
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    } else {
                        println!("{}", "Persistence not available".red());
                    }
                }
                "status" => {
                    if let Some(ref store) = state.persist_store {
                        match store.load_active_goals() {
                            Ok(active) => {
                                match store.load_all_goals() {
                                    Ok(all) => {
                                        let completed = all.iter().filter(|g| g.status == soul_persist::GoalStatus::Completed).count();
                                        let failed = all.iter().filter(|g| g.status == soul_persist::GoalStatus::Failed).count();
                                        println!("{}:", "Goal Stats".cyan().bold());
                                        println!("  Active: {}", active.len().to_string().green());
                                        println!("  Completed: {}", completed.to_string().cyan());
                                        println!("  Failed: {}", failed.to_string().red());
                                    }
                                    Err(e) => println!("{}: {}", "Error".red().bold(), e),
                                }
                            }
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    } else {
                        println!("{}", "Persistence not available".red());
                    }
                }
                _ => println!("{}", "Usage: goal [add|list|status]".yellow()),
            }
        }

        // ── Skills ──
        "skills" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "list" | "" => {
                    let skills = soul_skills::builtin_skills();
                    println!("{}", "Available Skills:".cyan().bold());
                    for s in &skills {
                        println!(
                            "  {} [pri={}] - {}",
                            s.name.green().bold(),
                            s.priority,
                            s.description
                        );
                        if !s.triggers.is_empty() {
                            println!("    triggers: {}", s.triggers.join(", ").dimmed());
                        }
                    }
                }
                "show" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: skills show <name>".yellow());
                    } else {
                        let name = sub_args[1];
                        let skills = soul_skills::builtin_skills();
                        if let Some(s) = skills.iter().find(|s| s.name == name) {
                            println!("{}", s.to_prompt());
                        } else {
                            println!("{}: skill '{}' not found", "Error".red(), name);
                        }
                    }
                }
                "match" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: skills match <input>".yellow());
                    } else {
                        let matches = state.skill_loader.find_matching(sub_args[1]);
                        if matches.is_empty() {
                            println!("{}", "No matching skills".yellow());
                        } else {
                            for s in &matches {
                                println!(
                                    "  {} (pri={})",
                                    s.name.green().bold(),
                                    s.priority
                                );
                            }
                        }
                    }
                }
                _ => println!("{}", "Usage: skills [list|show|match]".yellow()),
            }
        }

        // ── Knowledge Graph ──
        "graph" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "add" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: graph add <type> <label> [content]".yellow());
                    } else {
                        let parts: Vec<&str> = sub_args[1].splitn(3, ' ').collect();
                        let node_type = match parts[0].to_lowercase().as_str() {
                            "concept" => NodeType::Concept,
                            "decision" => NodeType::Decision,
                            "bug" => NodeType::Bug,
                            "task" => NodeType::Task,
                            "code" => NodeType::Code,
                            "document" => NodeType::Document,
                            "tool" => NodeType::Tool,
                            "error" => NodeType::Error,
                            _ => NodeType::Other(parts[0].into()),
                        };
                        let label = parts.get(1).unwrap_or(&"");
                        let mut node = Node::new(node_type, label);
                        if let Some(content) = parts.get(2) {
                            node = node.with_content(content);
                        }
                        let id = state.graph.add_node(node);
                        println!("{}: node '{}' added (id: {})", "✓".green(), label, &id[..8]);
                    }
                }
                "connect" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: graph connect <source_id> <target_id> <relation>".yellow());
                    } else {
                        let parts: Vec<&str> = sub_args[1].splitn(3, ' ').collect();
                        if parts.len() < 3 {
                            println!("{}", "Usage: graph connect <source_id> <target_id> <relation>".yellow());
                        } else {
                            let edge_type = match parts[2].to_lowercase().as_str() {
                                "depends_on" => EdgeType::DependsOn,
                                "caused_by" => EdgeType::CausedBy,
                                "solved_by" => EdgeType::SolvedBy,
                                "implements" => EdgeType::Implements,
                                "related_to" => EdgeType::RelatedTo,
                                "uses" => EdgeType::Uses,
                                "blocks" => EdgeType::Blocks,
                                _ => EdgeType::Other(parts[2].into()),
                            };
                            match state.graph.add_edge(Edge::new(parts[0], parts[1], edge_type)) {
                                Ok(_) => println!("{}: edge created", "✓".green()),
                                Err(e) => println!("{}: {}", "Error".red(), e),
                            }
                        }
                    }
                }
                "find" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: graph find <query>".yellow());
                    } else {
                        let results = state.graph.find_nodes_by_label(sub_args[1]);
                        if results.is_empty() {
                            println!("{}", "No matching nodes".yellow());
                        } else {
                            for n in results {
                                let edges_out = state.graph.edges_from(&n.id).len();
                                let edges_in = state.graph.edges_to(&n.id).len();
                                println!(
                                    "  {} [{}] {} (out={}, in={})",
                                    &n.id[..8],
                                    format!("{:?}", n.node_type).dimmed(),
                                    n.label.green(),
                                    edges_out,
                                    edges_in
                                );
                            }
                        }
                    }
                }
                "stats" => {
                    let stats = state.graph.stats();
                    println!("{}", "Graph Statistics:".cyan().bold());
                    println!("  Nodes: {}", stats.node_count);
                    println!("  Edges: {}", stats.edge_count);
                    println!("  Cycle: {}", stats.has_cycle);
                    for (t, c) in &stats.type_counts {
                        println!("    {}: {}", t, c);
                    }
                }
                _ => println!("{}", "Usage: graph [add|connect|find|stats]".yellow()),
            }
        }

        // ── Design Tree ──
        "design" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "new" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: design new <name> <description>".yellow());
                    } else {
                        let parts: Vec<&str> = sub_args[1].splitn(2, ' ').collect();
                        let name = parts[0];
                        let desc = parts.get(1).unwrap_or(&"");
                        let id = state.design_tree.create_node(name, desc);
                        println!("{}: design '{}' created (id: {})", "✓".green(), name, &id[..8]);
                    }
                }
                "list" => {
                    let all_nodes: Vec<_> = state.design_tree.find_by_state(&DesignState::Idea)
                        .into_iter()
                        .chain(state.design_tree.find_by_state(&DesignState::Research))
                        .chain(state.design_tree.find_by_state(&DesignState::Decision))
                        .chain(state.design_tree.find_by_state(&DesignState::Spec))
                        .chain(state.design_tree.find_by_state(&DesignState::Implementation))
                        .chain(state.design_tree.find_by_state(&DesignState::Testing))
                        .chain(state.design_tree.find_by_state(&DesignState::Verified))
                        .collect();
                    if all_nodes.is_empty() {
                        println!("{}", "No design nodes".yellow());
                    } else {
                        println!("{}", "Design Nodes:".cyan().bold());
                        for n in &all_nodes {
                            println!(
                                "  {} [{}] {}",
                                &n.id[..8],
                                format!("{:?}", n.state).green(),
                                n.name
                            );
                        }
                    }
                }
                "state" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: design state <id> <state>".yellow());
                    } else {
                        let parts: Vec<&str> = sub_args[1].splitn(2, ' ').collect();
                        if parts.len() < 2 {
                            println!("{}", "Usage: design state <id> <state>".yellow());
                        } else {
                            let state_val = match parts[1].to_lowercase().as_str() {
                                "idea" => DesignState::Idea,
                                "research" => DesignState::Research,
                                "decision" => DesignState::Decision,
                                "spec" => DesignState::Spec,
                                "implementation" => DesignState::Implementation,
                                "testing" => DesignState::Testing,
                                "verified" => DesignState::Verified,
                                "abandoned" => DesignState::Abandoned,
                                _ => {
                                    println!("{}: unknown state '{}'", "Error".red(), parts[1]);
                                    return;
                                }
                            };
                            let full_id = parts[0].to_string();
                            match state.design_tree.transition_node(&full_id, state_val, None) {
                                Ok(_) => println!("{}: state updated", "✓".green()),
                                Err(e) => println!("{}: {}", "Error".red(), e),
                            }
                        }
                    }
                }
                "stats" => {
                    let stats = state.design_tree.stats();
                    println!("{}", "Design Tree Statistics:".cyan().bold());
                    println!("  Total: {}", stats.total);
                    println!("  Active: {}", stats.active);
                    println!("  Completed: {}", stats.completed);
                    println!("  Abandoned: {}", stats.abandoned);
                    for (s, c) in &stats.state_counts {
                        println!("    {}: {}", s, c);
                    }
                }
                _ => println!("{}", "Usage: design [new|list|state|stats]".yellow()),
            }
        }

        // ── Inference Control ──
        "infer" => {
            if args.is_empty() {
                println!("{}", "Usage: infer <task description>".yellow());
            } else {
                let profile = InferenceController::select_profile(args);
                println!("{}", "Inference Profile:".cyan().bold());
                println!("  Capability: {:?}", profile.capability);
                println!("  Thinking:   {:?}", profile.thinking);
                println!("  Context:    {:?}", profile.context);
                println!(
                    "  Est. cost:  ${:.4}",
                    profile.estimated_cost_per_query()
                );
                println!(
                    "  Est. latency: {}ms",
                    profile.estimated_latency_ms()
                );
                println!("  Models: {:?}", profile.capability.models());
            }
        }

        // ── Critique ──
        "critique" => {
            if args.is_empty() {
                println!("{}", "Usage: critique <output to evaluate>".yellow());
            } else {
                let result = quick_critique(args, args);
                println!("{}", result.to_feedback());
            }
        }

        // ── MCP (Model Context Protocol) ──
        "mcp" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "tools" | "list" => {
                    println!("{}", "SoulSystem MCP Tools:".cyan().bold());
                    let handler = soul_mcp::FnMcpHandler::new(|_name, _args| {
                        Box::pin(async { Ok(serde_json::json!({"ok": true})) })
                    });
                    let server = soul_mcp::create_soul_mcp_server("soulsystem", Box::new(handler));
                    for t in server.tools() {
                        println!("  {} - {}", t.name.green().bold(), t.description);
                    }
                }
                "call" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: mcp call <tool_name> <args_json>".yellow());
                    } else {
                        let parts: Vec<&str> = sub_args[1].splitn(2, ' ').collect();
                        let tool_name = parts[0];
                        let args_json: serde_json::Value = parts.get(1)
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(serde_json::json!({}));

                        let handler = soul_mcp::FnMcpHandler::new(|name, args| {
                            Box::pin(async move {
                                match name.as_str() {
                                    "execute_shell" => {
                                        let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("echo no-op");
                                        let output = std::process::Command::new("sh").arg("-c").arg(cmd).output()
                                            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                                            .unwrap_or_else(|e| format!("Error: {e}"));
                                        Ok(serde_json::json!({ "output": output }))
                                    }
                                    "read_file" => {
                                        let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("/dev/null");
                                        let content = std::fs::read_to_string(path)
                                            .unwrap_or_else(|e| format!("Error: {e}"));
                                        Ok(serde_json::json!({ "content": content }))
                                    }
                                    _ => Err(soul_mcp::McpError::ToolNotFound(name)),
                                }
                            })
                        });
                        match handler.execute(tool_name, args_json).await {
                            Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    }
                }
                "server" => {
                    println!("{}", "Starting MCP server on stdio...".cyan());
                    println!("  (Connect an MCP client to interact)");
                    let handler = soul_mcp::FnMcpHandler::new(|_name, _args| {
                        Box::pin(async { Ok(serde_json::json!({"ok": true})) })
                    });
                    let server = soul_mcp::create_soul_mcp_server("soulsystem", Box::new(handler));
                    println!("  Server name: soulsystem v0.1.0");
                    println!("  Tools: {}", server.tool_count());
                }
                "connect" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: mcp connect <url>".yellow());
                        println!("  Example: mcp connect ws://localhost:8080/mcp");
                    } else {
                        let url = sub_args[1];
                        println!("{}: connecting to {}...", "●".cyan(), url);
                        let transport = soul_mcp::WsTransport::new(url);
                        match transport.connect_client().await {
                            Ok((tx, rx)) => {
                                println!("{}: connected to {}", "✓".green(), url);
                                let client = soul_mcp::McpClient::new(tx, rx);
                                match client.initialize(soul_mcp::McpServerInfo {
                                    name: "soulsystem-repl".into(),
                                    version: "0.1.0".into(),
                                }).await {
                                    Ok(caps) => {
                                        println!("  Server capabilities: tools={}, resources={}, prompts={}", caps.tools, caps.resources, caps.prompts);
                                        match client.list_tools().await {
                                            Ok(tools) => {
                                                println!("  Remote tools ({}):", tools.len());
                                                for t in &tools {
                                                    println!("    {} - {}", t.name.green(), t.description);
                                                }
                                            }
                                            Err(e) => println!("  Failed to list tools: {}", e),
                                        }
                                    }
                                    Err(e) => println!("  Init failed: {}", e),
                                }
                            }
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    }
                }
                "ws" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: mcp ws <addr> (start WebSocket MCP server)".yellow());
                        println!("  Example: mcp ws 127.0.0.1:9099");
                    } else {
                        let addr = sub_args[1];
                        println!("{}: starting WebSocket MCP server on {}...", "●".cyan(), addr);
                        let handler = soul_mcp::FnMcpHandler::new(|name, args| {
                            Box::pin(async move {
                                match name.as_str() {
                                    "execute_shell" => {
                                        let cmd = args.get("command").and_then(|c| c.as_str()).unwrap_or("echo no-op");
                                        let output = std::process::Command::new("sh").arg("-c").arg(cmd).output()
                                            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                                            .unwrap_or_else(|e| format!("Error: {e}"));
                                        Ok(serde_json::json!({ "output": output }))
                                    }
                                    "read_file" => {
                                        let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("/dev/null");
                                        let content = std::fs::read_to_string(path)
                                            .unwrap_or_else(|e| format!("Error: {e}"));
                                        Ok(serde_json::json!({ "content": content }))
                                    }
                                    _ => Err(soul_mcp::McpError::ToolNotFound(name)),
                                }
                            })
                        });
                        let server = soul_mcp::create_soul_mcp_server("soulsystem", Box::new(handler));
                        let transport = soul_mcp::WsTransport::new(addr);
                        println!("  Server started. Press Ctrl+C to stop.");
                        transport.serve_ws(server).await.ok();
                    }
                }
                _ => println!("{}", "Usage: mcp [tools|call|server|connect|ws]".yellow()),
            }
        }

        // ── Conversations ──
        "conversations" | "conv" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "new" | "create" => {
                    let title = if sub_args.len() > 1 && !sub_args[1].is_empty() {
                        sub_args[1]
                    } else {
                        "REPL Session"
                    };
                    if let Some(ref conv) = state.conversations {
                        match conv.create_session(title, Some("qwen3:8b")) {
                            Ok(sid) => {
                                state.session_id = Some(sid.clone());
                                println!("{}: session {} created", "✓".green(), sid);
                            }
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    } else {
                        println!("{}", "Conversations not initialized (store unavailable)".yellow());
                    }
                }
                "list" | "ls" => {
                    if let Some(ref conv) = state.conversations {
                        match conv.list_sessions() {
                            Ok(sessions) => {
                                if sessions.is_empty() {
                                    println!("{}", "No conversations yet".dimmed());
                                } else {
                                    println!("{}", "Conversations:".cyan().bold());
                                    for s in &sessions {
                                        let active = if state.session_id.as_deref() == Some(&s.id) { " ◀ active" } else { "" };
                                        println!("  {} [{}] {} ({} messages){}",
                                            s.id.green(), s.created_at.format("%Y-%m-%d %H:%M"),
                                            s.title, s.message_count, active);
                                    }
                                }
                            }
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    } else {
                        println!("{}", "Conversations not initialized".yellow());
                    }
                }
                "active" => {
                    if let Some(ref sid) = state.session_id {
                        println!("{}: session {}", "Active".cyan(), sid);
                    } else {
                        println!("{}", "No active session (use: conv new)".yellow());
                    }
                }
                "switch" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: conv switch <session_id>".yellow());
                    } else if let Some(ref conv) = state.conversations {
                        let sid = sub_args[1];
                        match conv.get_messages(sid) {
                            Ok(_) => {
                                state.session_id = Some(sid.to_string());
                                println!("{}: switched to session {}", "✓".green(), sid);
                            }
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    }
                }
                "stats" => {
                    if let Some(ref conv) = state.conversations {
                        match conv.stats() {
                            Ok(s) => println!("{}", serde_json::to_string_pretty(&s).unwrap()),
                            Err(e) => println!("{}: {}", "Error".red(), e),
                        }
                    }
                }
                _ => println!("{}", "Usage: conv [new|list|active|switch|stats]".yellow()),
            }
        }

        // ── Daemon events ──
        "events" => {
            if let Some(ref store) = state.persist_store {
                match store.load_recent_actions(20) {
                    Ok(actions) => {
                        if actions.is_empty() {
                            println!("{}", "No events recorded".dimmed());
                        } else {
                            println!("{}:", "Recent Events".cyan().bold());
                            for action in &actions {
                                let icon = if action.success { "✓".green() } else { "✗".red() };
                                println!(
                                    "  {} {} → {} ({})",
                                    icon,
                                    action.action.dimmed(),
                                    if action.result.len() > 80 {
                                        format!("{}...", &action.result[..80])
                                    } else {
                                        action.result.clone()
                                    },
                                    action.timestamp.format("%H:%M:%S")
                                );
                            }
                        }
                    }
                    Err(e) => println!("{}: {}", "Error".red().bold(), e),
                }
            } else {
                println!("{}", "Persistence not available".red());
            }
        }

        // ── Live daemon events ──
        "daemon" => {
            if let Some(ref mut rx) = state.daemon_rx {
                println!("{}", "Listening for daemon events (Ctrl+C to stop)...".cyan().bold());
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let icon = match event.kind.as_str() {
                                "TaskReceived" => "→".green(),
                                "TaskCompleted" => "✓".green().bold(),
                                "TaskFailed" => "✗".red(),
                                "Thought" => "●".cyan(),
                                "ToolCall" => "→".yellow(),
                                "ToolResult" => "←".dimmed(),
                                "SafetyWarning" => "⚠".yellow().bold(),
                                _ => "•".dimmed(),
                            };
                            println!(
                                "  {} [{}] {} ({})",
                                icon,
                                event.source,
                                event.kind,
                                event.timestamp
                            );
                        }
                        Err(e) => {
                            println!("{}: {}", "Error".red().bold(), e);
                            break;
                        }
                    }
                }
            } else {
                println!("{}", "No daemon connected. Start with: soulsystem --daemon --repl".yellow());
            }
        }

        // ── Sub-Agents ──
        "subagent" | "sa" => {
            let sub_args: Vec<&str> = args.splitn(2, ' ').collect();
            match sub_args[0] {
                "spawn" => {
                    if sub_args.len() < 2 || sub_args[1].is_empty() {
                        println!("{}", "Usage: subagent spawn <description>".yellow());
                    } else {
                        let desc = sub_args[1];
                        match state.sub_agents.spawn(desc, None).await {
                            Ok(id) => println!("{}: spawned sub-agent {}", "✓".green(), id.green()),
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    }
                }
                "list" | "ls" => {
                    let tasks = state.sub_agents.list_tasks().await;
                    if tasks.is_empty() {
                        println!("{}", "No sub-agent tasks".dimmed());
                    } else {
                        println!("{}", "Sub-Agent Tasks:".cyan().bold());
                        for t in &tasks {
                            let status = match t.status {
                                soul_subagents::TaskStatus::Pending => "⏳".yellow(),
                                soul_subagents::TaskStatus::Running => "▶".cyan(),
                                soul_subagents::TaskStatus::Completed => "✓".green(),
                                soul_subagents::TaskStatus::Failed => "✗".red(),
                                soul_subagents::TaskStatus::Cancelled => "○".dimmed(),
                            };
                            println!("  {} [{}] {} {}",
                                status, &t.id[..8], t.description,
                                if let Some(ref r) = t.result { format!("→ {}", truncate_repl(r, 50)) } else { String::new() });
                        }
                    }
                }
                "status" => {
                    if sub_args.len() < 2 {
                        println!("{}", "Usage: subagent status <id>".yellow());
                    } else {
                        match state.sub_agents.get_task(sub_args[1]).await {
                            Ok(t) => {
                                println!("Task: {} ({})", t.id.green(), t.description);
                                println!("  Status: {:?}", t.status);
                                println!("  Created: {}", t.created_at);
                                if let Some(r) = &t.result { println!("  Result: {}", truncate_repl(r, 100)); }
                                if let Some(e) = &t.error { println!("  Error: {}", e.red()); }
                            }
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    }
                }
                "cancel" => {
                    if sub_args.len() < 2 {
                        println!("{}", "Usage: subagent cancel <id>".yellow());
                    } else {
                        match state.sub_agents.cancel(sub_args[1]).await {
                            Ok(()) => println!("{}: cancelled {}", "✓".green(), sub_args[1]),
                            Err(e) => println!("{}: {}", "Error".red().bold(), e),
                        }
                    }
                }
                "results" => {
                    let results = state.sub_agents.collect_results().await;
                    if results.is_empty() {
                        println!("{}", "No completed results".dimmed());
                    } else {
                        println!("{}", "Sub-Agent Results:".cyan().bold());
                        for (id, result) in &results {
                            println!("  {} → {}", id.green(), truncate_repl(result, 80));
                        }
                    }
                }
                _ => println!("{}", "Usage: subagent <spawn|list|status|cancel|results>".yellow()),
            }
        }

        _ => {
            println!(
                "{}: unknown command '{}'. Type 'help' for commands.",
                "Error".red(),
                cmd
            );
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

fn truncate_repl(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
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
    println!("  {} - Toggle verbose output", "verbose".green());
    println!();
    println!("{}", "Persistence:".cyan().bold());
    println!("  {} - Save context to disk", "save".green());
    println!("  {} - Load context from disk", "load".green());
    println!();
    println!("{}", "Sub-Agents:".cyan().bold());
    println!("  {} - Spawn a sub-agent for a task", "subagent spawn <desc>".green());
    println!("  {} - List all sub-agent tasks", "subagent list".green());
    println!("  {} - Show sub-agent status", "subagent status <id>".green());
    println!("  {} - Cancel a sub-agent", "subagent cancel <id>".green());
    println!("  {} - Collect results", "subagent results".green());
    println!();
    println!("{}", "Meta:".cyan().bold());
    println!("  {} - Show this help", "help".green());
    println!("  {} - Exit", "exit".green());
}
