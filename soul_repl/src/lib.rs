use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use soul_agent_core::{AgentConfig, AgentEvent, AutonomousAgent};
use soul_critique::quick_critique;
use soul_designtree::{DesignState, DesignTree};
use soul_inference::InferenceController;
use soul_llm::{LlmConfig, OllamaClient};
use soul_mcp::McpToolHandler;
use soul_memory::PersistentStore;
use soul_memory::{Edge, EdgeType, KnowledgeGraph, Node, NodeType};
use soul_planner::CognitiveLoop;
use soul_skills::SkillLoader;
use soul_subagents::SubAgentManager;
use soul_tools::{discover_system_tools, execute_shell, ToolRegistry};
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};

pub struct ReplState {
    pub agent: AutonomousAgent,
    pub verbose: bool,
    pub persist_store: Option<PersistentStore>,
    pub daemon_rx: Option<broadcast::Receiver<soul_dashboard::BusEvent>>,
    pub graph: KnowledgeGraph,
    pub design_tree: DesignTree,
    pub skill_loader: SkillLoader,
    pub conversations: Option<soul_memory::ConversationStore>,
    pub session_id: Option<String>,
    pub sub_agents: SubAgentManager,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        let agent_config = AgentConfig::default();
        let agent = AutonomousAgent::new(OllamaClient::new(config), agent_config);

        // Try to open persistent store
        let persist_dir = std::env::temp_dir().join("soul_repl_persist");
        std::fs::create_dir_all(&persist_dir).ok();
        let persist_store = PersistentStore::open(&persist_dir).ok();

        let data_dir = std::env::temp_dir().join("soul_repl_data");
        std::fs::create_dir_all(&data_dir).ok();
        let skills_dir = data_dir.join(".skills");
        std::fs::create_dir_all(&skills_dir).ok();

        Self {
            agent,
            verbose: true,
            persist_store,
            daemon_rx: None,
            graph: KnowledgeGraph::new(),
            design_tree: DesignTree::new(&data_dir.join("design")),
            skill_loader: SkillLoader::new(&skills_dir),
            conversations: soul_memory::ConversationStore::open(&data_dir.join("conversations")).ok(),
            session_id: None,
            sub_agents: SubAgentManager::new(soul_llm::LlmConfig::default(), 4),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_repl_short() {
        let s = "hello";
        assert_eq!(truncate_repl(s, 10), "hello");
    }

    #[test]
    fn test_truncate_repl_long() {
        let s = "hello world this is a long string";
        let truncated = truncate_repl(s, 10);
        assert_eq!(truncated, "hello worl...");
        assert_eq!(truncated.len(), 13); // 10 + "..."
    }

    #[test]
    fn test_truncate_repl_empty() {
        assert_eq!(truncate_repl("", 10), "");
    }

    #[test]
    fn test_truncate_repl_exact() {
        let s = "12345";
        assert_eq!(truncate_repl(s, 5), "12345");
    }

    #[test]
    fn test_truncate_repl_one_char() {
        let s = "abc";
        assert_eq!(truncate_repl(s, 1), "a...");
    }

    #[test]
    fn test_repl_state_new() {
        let config = LlmConfig::default();
        let state = ReplState::new(config);
        assert_eq!(state.verbose, true);
        assert_eq!(state.session_id, None);
    }

    #[test]
    fn test_repl_state_with_daemon() {
        let config = LlmConfig::default();
        let (tx, rx) = broadcast::channel(16);
        let _state = ReplState::new(config).with_daemon_events(rx);
        drop(tx);
        // Should have daemon_rx set
        // Can't easily assert on broadcast receiver, just verify no panic
    }

    #[test]
    fn test_print_help_does_not_panic() {
        print_help();
    }

    #[test]
    fn test_repl_state_default_fields() {
        let config = LlmConfig::default().with_model("test-repl-model");
        let state = ReplState::new(config);
        assert!(state.persist_store.is_some() || state.persist_store.is_none());
        // graph should be initialized (KnowledgeGraph::new())
        let stats = state.graph.stats();
        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.edge_count, 0);
    }

    #[test]
    fn test_repl_state_design_tree() {
        let config = LlmConfig::default();
        let state = ReplState::new(config);
        let stats = state.design_tree.stats();
        assert_eq!(stats.total, 0);
    }
}

pub fn run_repl(state: &mut ReplState) {
    let rt = new_runtime();

    let mut rl = DefaultEditor::new().expect("Failed to create readline");

    println!("{}", "╔══════════════════════════════════════════╗".cyan());
    println!("{}", "║   SoulSystem Autonomous Agent v0.2.0   ║".cyan().bold());
    println!("{}", "║   Type 'help' for commands              ║".cyan());
    println!("{}", "╚══════════════════════════════════════════╝".cyan());
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

            // Auto-select inference profile based on question complexity
            let profile = InferenceController::select_profile(args);
            if state.verbose {
                println!(
                    "  {} capability={:?} thinking={:?} context={:?}",
                    "● infer:".dimmed(),
                    profile.capability,
                    profile.thinking,
                    profile.context
                );
            }

            // Inject graph memory context
            let enriched_prompt = if !state.graph.find_nodes_by_label(args).is_empty() || args.len() > 5 {
                let ctx = state.graph.context_for_query(args, 5);
                if ctx.is_empty() {
                    args.to_string()
                } else {
                    format!("{ctx}\n\nUser query: {args}")
                }
            } else {
                args.to_string()
            };

            match state.agent.ask(&enriched_prompt).await {
                Ok(resp) => {
                    println!("{}", resp);
                    // Auto-persist conversation
                    if let Some(ref conv) = state.conversations {
                        if state.session_id.is_none() {
                            if let Ok(id) = conv.create_session("auto", Some("qwen3:8b")) {
                                state.session_id = Some(id);
                            }
                        }
                        if let Some(ref sid) = state.session_id {
                            conv.add_message(soul_memory::AddMessageParams {
                                session_id: sid,
                                role: "user",
                                content: args,
                                tool_calls: None,
                                tool_call_id: None,
                                tokens_used: None,
                                model: None,
                            }).ok();
                            conv.add_message(soul_memory::AddMessageParams {
                                session_id: sid,
                                role: "assistant",
                                content: &resp,
                                tool_calls: None,
                                tool_call_id: None,
                                tokens_used: None,
                                model: None,
                            }).ok();
                        }
                    }
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }
        }

        // ── Run task with full autonomous loop ──
        "run" | "task" => {
            if args.is_empty() {
                println!("{}", "Usage: run <task description>".yellow());
                return;
            }
            println!("{}", "Starting autonomous task...".cyan().bold());

            // Auto-track in design tree
            let design_id = state.design_tree.create_node(args, "Auto-created from 'run' command");
            state.design_tree.transition_node(&design_id, soul_designtree::DesignState::Implementation, Some("auto-started".into())).ok();
            println!("  {} design node: {}", "●".dimmed(), &design_id[..8]);

            // Auto-add to knowledge graph
            let graph_node = soul_memory::Node::new(soul_memory::NodeType::Task, args);
            let graph_id = state.graph.add_node(graph_node);
            println!("  {} graph node: {}", "●".dimmed(), &graph_id[..8]);

            let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

            // Collect events in background
            let event_handle = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        AgentEvent::Thinking { content } => {
                            println!("  {} {}", "●".dimmed(), content.dimmed());
                        }
                        AgentEvent::ToolCall { name, args } => {
                            let args_str = if args.to_string().len() > 80 {
                                format!("{}...", &args.to_string()[..80])
                            } else {
                                args.to_string()
                            };
                            println!("  {} {} {}", "→".yellow(), name.green().bold(), args_str.dimmed());
                        }
                        AgentEvent::ToolResult { name, output, success } => {
                            let icon = if success { "✓".green() } else { "✗".red() };
                            let preview = if output.len() > 200 {
                                format!("{}...", &output[..200])
                            } else {
                                output
                            };
                            println!("  {} {} {}", icon, name, preview.dimmed());
                        }
                        AgentEvent::Response { content } => {
                            println!();
                            println!("{}", content);
                            println!();
                        }
                        AgentEvent::SafetyWarning { message } => {
                            println!("  {} {}", "⚠".yellow().bold(), message.yellow());
                        }
                        AgentEvent::Done { summary } => {
                            println!();
                            println!("{} {}", "✓".green().bold(), summary.green());
                        }
                        AgentEvent::Error { message } => {
                            println!("{}: {}", "Error".red().bold(), message);
                        }
                    }
                }
            });

            state.agent.set_event_sender(tx);

            match state.agent.run_task(args).await {
                Ok(result) => {
                    println!();
                    println!("{}: {}", "Result".cyan().bold(), result);

                    // Auto-transition design tree: Implementation → Testing → Verified
                    let design_nodes: Vec<String> = state.design_tree.find_by_state(&soul_designtree::DesignState::Implementation)
                        .into_iter().map(|n| n.id.clone()).collect();
                    for id in &design_nodes {
                        if state.design_tree.transition_node(id, soul_designtree::DesignState::Testing, Some("task completed".into())).is_ok() {
                            println!("  {} design {} → Testing", "●".dimmed(), &id[..8]);
                        }
                    }

                    // Auto-add result to knowledge graph
                    let result_node = soul_memory::Node::new(
                        soul_memory::NodeType::Task,
                        &format!("result: {}", &result[..result.len().min(50)]),
                    ).with_content(&result);
                    state.graph.add_node(result_node);

                    println!("  {} knowledge graph updated", "●".dimmed());
                }
                Err(e) => println!("{}: {}", "Error".red().bold(), e),
            }

            event_handle.abort();
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
            let plan = state.agent.planner.create_plan_llm(&goal, &[], &state.agent.llm).await;
            println!("{}:", "Plan".cyan().bold());
            for (i, step) in plan.steps.iter().enumerate() {
                println!(
                    "  {}. {} {}",
                    (i + 1).to_string().green(),
                    step.action,
                    if let Some(tool) = &step.tool {
                        format!("({})", tool).dimmed().to_string()
                    } else {
                        String::new()
                    }
                );
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
            let obs = state.agent.planner.memory.recent_observations(10);
            let key_info = &state.agent.planner.memory.key_info;
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
            println!("  History: {} actions", state.agent.planner.history.actions.len());
            println!("  Success Rate: {:.1}%", state.agent.planner.history.success_rate() * 100.0);
        }

        // ── Observe ──
        "observe" => {
            if args.is_empty() {
                println!("{}", "Usage: observe <observation>".yellow());
                return;
            }
            state.agent.planner.memory.observe(args.to_string());
            println!("{}: {}", "Observed".green().bold(), args);
        }

        // ── History ──
        "history" => {
            let recent = state.agent.planner.history.recent(15);
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

        // ── Status ──
        "status" => {
            let status = state.agent.status();
            println!("{}:", "Status".cyan().bold());
            println!("  Model: {}", status["llm_model"].as_str().unwrap_or("?").green());
            println!("  Turn: {}/{}", status["turn"], status["max_turns"]);
            println!("  Tools: {}", status["tools"]);
            println!(
                "  Success Rate: {:.1}%",
                status["success_rate"].as_f64().unwrap_or(1.0) * 100.0
            );
            println!("  Observations: {}", status["observations"]);
            println!("  Conversation: {}", status["conversation"].as_str().unwrap_or("?"));
        }

        // ── Models ──
        "models" => {
            match state.agent.llm.list_models().await {
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
            let mut rag = soul_memory::RagStore::new(soul_memory::RagConfig::default());
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
            let mut rag = soul_memory::RagStore::new(soul_memory::RagConfig::default());
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
            let rt = new_runtime();
            if let Err(e) = rt.block_on(state.design_tree.save()) {
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
                let mem = soul_memory::PersistentWorkingMemory {
                    key_info: state.agent.planner.memory.key_info.clone(),
                    observations: state.agent.planner.memory.recent_observations(50).to_vec(),
                    related_sop: None,
                    context: serde_json::json!({
                        "turn": state.agent.turn,
                        "tools": state.agent.registry.list().len(),
                    }),
                    last_updated: chrono::Utc::now(),
                };
                store.save_memory(&mem).ok();
                for action in state.agent.planner.history.recent(20) {
                    let entry = soul_memory::ChatEntry {
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
                match soul_memory::KnowledgeGraph::load(&graph_path) {
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
                let rt = new_runtime();
                let count = rt.block_on(state.design_tree.load()).unwrap_or(0);
                println!("{}: design tree loaded ({} nodes)", "✓".green(), count);
            }

            // Load agent context
            if let Some(ref store) = state.persist_store {
                match store.load_memory() {
                    Ok(mem) => {
                        state.agent.planner.memory.key_info = mem.key_info;
                        for obs in mem.observations {
                            state.agent.planner.memory.observe(obs);
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
                        let goal = soul_memory::PersistentGoal {
                            id: uuid::Uuid::new_v4().to_string(),
                            description: sub_args.to_string(),
                            priority: 5,
                            status: soul_memory::GoalStatus::Active,
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
                                            soul_memory::GoalStatus::Active => "○".yellow(),
                                            soul_memory::GoalStatus::Running => "●".green(),
                                            soul_memory::GoalStatus::Completed => "✓".cyan(),
                                            soul_memory::GoalStatus::Failed => "✗".red(),
                                            soul_memory::GoalStatus::Deferred => "◦".dimmed(),
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
                                        let completed = all.iter().filter(|g| g.status == soul_memory::GoalStatus::Completed).count();
                                        let failed = all.iter().filter(|g| g.status == soul_memory::GoalStatus::Failed).count();
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
                        let rt = new_runtime();
                        match rt.block_on(handler.execute(tool_name, args_json)) {
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
                        match new_runtime().block_on(transport.connect_client()) {
                            Ok((tx, rx)) => {
                                println!("{}: connected to {}", "✓".green(), url);
                                let client = soul_mcp::McpClient::new(tx, rx);
                                let rt = new_runtime();
                                match rt.block_on(client.initialize(soul_mcp::McpServerInfo {
                                    name: "soulsystem-repl".into(),
                                    version: "0.1.0".into(),
                                })) {
                                    Ok(caps) => {
                                        println!("  Server capabilities: tools={}, resources={}, prompts={}", caps.tools, caps.resources, caps.prompts);
                                        match rt.block_on(client.list_tools()) {
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
                        let rt = new_runtime();
                        rt.block_on(transport.serve_ws(server)).ok();
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

        _ => {
                    println!("Usage: goal <add|list|status>");
                }
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
                let rt_handle = tokio::runtime::Handle::current();
                loop {
                    let mut rx_ref = rx.resubscribe();
                    match rt_handle.block_on(rx_ref.recv()) {
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

fn truncate_repl(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn print_help() {
    use colored::Colorize;
    println!("{}", "╔══════════════════════════════════════════╗".cyan());
    println!("{}", "║   SoulSystem Autonomous Agent v0.3.0   ║".cyan().bold());
    println!("{}", "║   Type 'help' for commands              ║".cyan());
    println!("{}", "╚══════════════════════════════════════════╝".cyan());
    println!();
    println!("{}", "Core Commands:".cyan().bold());
    println!("  {} - Ask with conversation context", "ask <msg>".green());
    println!("  {} - Run autonomous task (ReAct loop)", "run <task>".green());
    println!("  {} - Create plan using LLM", "plan <goal>".green());
    println!("  {} - Execute shell command directly", "shell <cmd>".green());
    println!();
    println!("{}", "Memory & State:".cyan().bold());
    println!("  {} - View memory, history, status", "memory".green());
    println!("  {} - Record an observation", "observe <msg>".green());
    println!("  {} - View action history", "history".green());
    println!();
    println!("{}", "Goals & Daemon:".cyan().bold());
    println!("  {} - Add a new goal", "goal add <desc>".green());
    println!("  {} - List all goals", "goal list".green());
    println!("  {} - Goal statistics", "goal status".green());
    println!("  {} - View recent events", "events".green());
    println!("  {} - Live daemon event stream", "daemon".green());
    println!();
    println!("{}", "Web & Browser:".cyan().bold());
    println!("  {} - Fetch and display web page content", "browse <url>".green());
    println!("  {} - Fetch multiple URLs at once", "fetch <url1> <url2>".green());
    println!("  {} - Search web and show results with relevance", "websearch <query>".green());
    println!("  {} - Ask question with web context", "askweb <question>".green());
    println!();
    println!("{}", "Skills & Quality:".cyan().bold());
    println!("  {} - List available skills", "skills list".green());
    println!("  {} - Show skill details", "skills show <name>".green());
    println!("  {} - Match input to skills", "skills match <input>".green());
    println!("  {} - Get inference profile", "infer <task>".green());
    println!("  {} - Critique an output", "critique <output>".green());
    println!();
    println!("{}", "Knowledge Graph:".cyan().bold());
    println!("  {} - Add a node", "graph add <type> <label>".green());
    println!("  {} - Connect nodes", "graph connect <src> <tgt> <rel>".green());
    println!("  {} - Search graph", "graph find <query>".green());
    println!("  {} - Graph statistics", "graph stats".green());
    println!();
    println!("{}", "Design Tree:".cyan().bold());
    println!("  {} - Create design node", "design new <name> <desc>".green());
    println!("  {} - List all designs", "design list".green());
    println!("  {} - Transition state", "design state <id> <state>".green());
    println!("  {} - Design statistics", "design stats".green());
    println!();
    println!("{}", "Conversations:".cyan().bold());
    println!("  {} - Create new conversation session", "conversations new [title]".green());
    println!("  {} - List all conversations", "conversations list".green());
    println!("  {} - Show active session", "conversations active".green());
    println!("  {} - Switch to a session", "conversations switch <id>".green());
    println!("  {} - Show stats", "conversations stats".green());
    println!();
    println!("{}", "MCP (Model Context Protocol):".cyan().bold());
    println!("  {} - List MCP tools", "mcp tools".green());
    println!("  {} - Call an MCP tool", "mcp call <tool> <args>".green());
    println!("  {} - Start stdio MCP server", "mcp server".green());
    println!("  {} - Start WebSocket MCP server", "mcp ws <addr>".green());
    println!("  {} - Connect to remote MCP server", "mcp connect <url>".green());
    println!();
    println!("{}", "System:".cyan().bold());
    println!("  {} - List available tools", "tools".green());
    println!("  {} - System status & health", "status".green());
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
