//! # soul_repl — REPL interactif pour l'entité autonome

use colored::Colorize;
use serde_json;

use soul_llm::{LlmConfig, OllamaClient};
use soul_planner::{CognitiveLoop, Goal, GoalStatus};
use soul_tools::ToolRegistry;

pub struct ReplState {
    pub llm: OllamaClient,
    pub planner: CognitiveLoop,
    pub tools: ToolRegistry,
    pub entity_name: String,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        let mut reg = ToolRegistry::new();
        for t in soul_tools::discover_system_tools() {
            reg.register(t);
        }
        Self {
            llm: OllamaClient::new(config),
            planner: CognitiveLoop::new(),
            tools: reg,
            entity_name: "soul".into(),
        }
    }

    pub async fn status(&self) -> String {
        let alive = self.llm.is_alive().await;
        let models = if alive {
            match self.llm.list_models().await {
                Ok(m) => m.join(", "),
                Err(_) => "erreur".into(),
            }
        } else {
            "Ollama non connecté".into()
        };
        let success_rate = self.planner.history.success_rate();
        let recent_obs = self.planner.memory.recent_observations(3);

        format!(
            "{}\n{}\n{}\n{}\n{}",
            "🧠 État de l'entité".bright_blue().bold(),
            format!("  Model: {}", models.bright_green()),
            format!("  Ollama: {} connecté", if alive { "✅" } else { "❌" }),
            format!("  Taux succès actions: {:.0}%", success_rate * 100.0),
            format!("  Dernières observations: {:?}", recent_obs)
        )
    }

    pub async fn run_command(&mut self, cmd: &str) -> String {
        let trimmed = cmd.trim();
        if trimmed.is_empty() { return "".into(); }
        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        match parts[0] {
            "ask" => self.cmd_ask(parts.get(1).map(|s| s.trim()).unwrap_or("")).await,
            "plan" => self.cmd_plan(parts.get(1).map(|s| s.trim()).unwrap_or("")),
            "run" => self.cmd_run(parts.get(1).map(|s| s.trim()).unwrap_or("")),
            "tools" => self.tools_list(),
            "memory" => self.memory_list(),
            "observe" => self.cmd_observe(parts.get(1).map(|s| s.trim()).unwrap_or("")),
            "decide" => self.cmd_decide(parts.get(1).map(|s| s.trim()).unwrap_or("")),
            "history" => self.cmd_history(),
            "models" => self.cmd_models().await,
            "help" => HELP.to_string(),
            "status" => self.status().await,
            "exit" | "quit" => "👋 Au revoir !".to_string(),
            _ => format!("❌ Commande inconnue. Tapez `help`."),
        }
    }

    async fn cmd_ask(&mut self, msg: &str) -> String {
        if msg.is_empty() { return "❌ ask <message>".into(); }
        match self.llm.generate(msg).await {
            Ok(resp) => resp.response.bright_cyan().to_string(),
            Err(e) => format!("❌ Erreur LLM: {}", e),
        }
    }

    fn cmd_plan(&mut self, goal_desc: &str) -> String {
        if goal_desc.is_empty() { return "❌ plan <objectif>".into(); }
        let goal = Goal {
            id: uuid::Uuid::new_v4().to_string(),
            description: goal_desc.to_string(),
            priority: 5,
            created_at: chrono::Utc::now(),
            status: GoalStatus::Active,
        };
        let plan = self.planner.create_plan(&goal, &["analyze".into(), "execute".into()]);
        format!(
            "✅ Plan créé pour \"{}\": {} étapes\n{}",
            goal.description.bright_yellow(),
            format!("{}", plan.steps.len()).bright_green(),
            plan.steps.iter().map(|s| format!("   - [{}] {}", s.order, s.command)).collect::<Vec<_>>().join("\n")
        )
    }

    fn cmd_run(&mut self, shell_cmd: &str) -> String {
        if shell_cmd.is_empty() { return "❌ run <commande_shell>".into(); }
        match soul_tools::execute_shell(shell_cmd) {
            Ok(output) => output.bright_white().to_string(),
            Err(e) => format!("❌ {}", e.bright_red()),
        }
    }

    fn tools_list(&self) -> String {
        let all = self.tools.all();
        if all.is_empty() { return "🔧 Aucun outil découvert.".into(); }
        all.iter().map(|t| format!("  {:20} {}", t.name.bright_yellow(), t.description)).collect::<Vec<_>>().join("\n")
    }

    fn memory_list(&self) -> String {
        let obs = self.planner.memory.recent_observations(10);
        if obs.is_empty() { return "🧠 Mémoire vide.".into(); }
        obs.iter().enumerate().map(|(i, o)| format!("  [{}] {}", i + 1, o.bright_white())).collect::<Vec<_>>().join("\n")
    }

    fn cmd_observe(&mut self, msg: &str) -> String {
        if msg.is_empty() { return "❌ observe <message>".into(); }
        self.planner.memory.observe(msg.to_string());
        format!("👁 Observé: {}", msg.bright_cyan())
    }

    fn cmd_decide(&self, ctx: &str) -> String {
        if ctx.is_empty() { return "❌ decide <contexte>".into(); }
        let decision = self.planner.decide(ctx);
        format!(
            "⚡ Décision: \"{}\"\n   Raisonnement: {}\n   Confiance: {:.0}%",
            decision.action.bright_yellow(), decision.reasoning, decision.confidence * 100.0
        )
    }

    fn cmd_history(&self) -> String {
        let recent = self.planner.history.recent(5);
        if recent.is_empty() { return "📭 Aucune action enregistrée".into(); }
        recent.iter().rev().map(|r| format!(
            "  [{}] {} {} → {}",
            r.timestamp.format("%H:%M"),
            if r.success { "✅" } else { "❌" },
            r.command.bright_yellow(),
            if r.result.len() > 60 { format!("{}...", &r.result[..60]) } else { r.result.clone() }
        )).collect::<Vec<_>>().join("\n")
    }

    async fn cmd_models(&self) -> String {
        match self.llm.list_models().await {
            Ok(models) => models.iter().map(|m| format!("  {}", m.bright_green())).collect::<Vec<_>>().join("\n"),
            Err(e) => format!("❌ {}", e),
        }
    }
}

const HELP: &str = r#"
🧠 SoulSystem REPL — Commandes

  ask <msg>       Poser une question au LLM
  plan <objectif> Créer un plan pour un objectif
  run <cmd>       Exécuter une commande shell
  tools           Lister les outils disponibles
  memory          Voir la mémoire de travail
  observe <msg>   Enregistrer une observation
  decide <ctx>    Prendre une décision
  history         Voir l'historique des actions
  status          État du système
  models          Lister les modèles Ollama
  help            Cette aide
  exit            Quitter
"#;

/// Alias utilisé par Editor
use rustyline::history::DefaultHistory as HistoryImpl;
type ReplEditor = rustyline::Editor<(), HistoryImpl>;

pub async fn run_repl(state: &mut ReplState) {
    let mut rl: ReplEditor = match ReplEditor::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    println!("{}", "🧠 SoulSystem REPL — Entité autonome".bright_blue().bold());
    println!("Tapez `help` pour les commandes.\n");

    loop {
        let prompt = format!("[{}] > ", state.entity_name.bright_cyan());
        match rl.readline(&prompt) {
            Ok(line) => {
                if line.is_empty() { continue; }
                let _ = rl.add_history_entry(line.as_str());
                let output = state.run_command(&line).await;
                println!("{}\n", output);
                if line.starts_with("exit") || line.starts_with("quit") { break; }
            }
            Err(_) => break,
        }
    }
}
