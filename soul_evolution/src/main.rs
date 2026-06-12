use soul_evolution::audit::{self, Auditor};
use soul_evolution::analyzer::Analyzer;
use soul_evolution::generator::Generator;
use soul_evolution::validator::Validator;
use soul_evolution::meta_evolution::MetaEvolver;
use soul_evolution::evol_loop::{self, SelfImprovementLoop};
use soul_evolution::optimizer::{Optimizer, OptimizationState, SYSTEM_PROMPT};
use soul_evolution::types::Severity;
use soul_evolution::types::EvolutionConfig;
use std::env;

const USAGE: &str = r#"
aevolve — Agent Evolution Engine (Recursive Self-Improvement Loop)

USAGE:
  aevolve <command> [options]

COMMANDS:
  audit         Scan and evaluate all agent definitions
  analyze       Detect gaps, bottlenecks, and improvement opportunities
  propose       Generate improvement proposals
  validate      Validate proposal files
  evolve        Run one full iteration of the self-improvement loop
  recursive     Run recursive iterations (loop calls itself until convergence)
  meta          Run the 7-pillar meta-evolution cycle (Godel/Darwin/STOP/etc.)
  status        Show current loop state and evolution history
  full-report   Run audit + analyze + propose in sequence
  optimize      Low-level Rust code optimization (System Prompt + Cases A/B/C)

  optimize subcommands:
    optimize prompt system       Show the system prompt
    optimize prompt case-a <e>   Format Case A (compile failure)
    optimize prompt case-b <e>   Format Case B (logic failure)
    optimize prompt case-c <t> <b>  Format Case C (success, t=time, b=best)
    optimize compile <code>      Compile Rust code and return result

OPTIONS:
  --agents-dir <path>     Agent directory (default: /root/.agents/agents)
  --apply                 Actually write generated files (default: dry-run)
  --max-iter <n>          Maximum recursive iterations (default: 1)
  --verbose               Enable verbose output
  --help                  Show this help message
"#;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        println!("{}", USAGE);
        return;
    }

    let command = &args[1];

    if command == &"optimize" {
        tracing_subscriber::fmt()
            .with_env_filter("soul_evolution=info")
            .with_target(false)
            .init();
        cmd_optimize(&args[2..]);
        return;
    }

    let mut config = EvolutionConfig::default();
    let mut apply = false;
    let mut max_iter: Option<u64> = None;
    let mut verbose = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--agents-dir" if i + 1 < args.len() => {
                config.agents_dir = std::path::PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--apply" => {
                apply = true;
                i += 1;
            }
            "--max-iter" if i + 1 < args.len() => {
                max_iter = Some(args[i + 1].parse().unwrap_or(1));
                i += 2;
            }
            "--verbose" => {
                verbose = true;
                i += 1;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                i += 1;
            }
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            if verbose {
                "soul_evolution=debug"
            } else {
                "soul_evolution=info"
            },
        )
        .with_target(false)
        .init();

    config.auto_apply = apply;

    match command.as_str() {
        "audit" => cmd_audit(&config),
        "analyze" => cmd_analyze(&config),
        "propose" => cmd_propose(&config),
        "validate" => cmd_validate(&config),
        "evolve" => cmd_evolve(config, apply, None, verbose),
        "recursive" => cmd_evolve(config, apply, max_iter.or(Some(3)), verbose),
        "meta" => cmd_meta(&config),
        "status" => cmd_status(),
        "full-report" => cmd_full_report(&config),
        "optimize" => cmd_optimize(&args[2..]),
        _ => {
            eprintln!("Unknown command: {}", command);
            println!("{}", USAGE);
            std::process::exit(1);
        }
    }
}

fn cmd_audit(config: &EvolutionConfig) {
    let audits = Auditor::audit_agents(config);
    let counts = Auditor::count_files(config);
    let report = Auditor::generate_report(config, &audits, &counts);
    println!("{}", audit::format_audit_report(&report));
}

fn cmd_analyze(config: &EvolutionConfig) {
    let audits = Auditor::audit_agents(config);
    let counts = Auditor::count_files(config);
    let report = Auditor::generate_report(config, &audits, &counts);
    let analysis = Analyzer::analyze(&report);
    println!("{}", Analyzer::format_analysis(&analysis));
}

fn cmd_propose(config: &EvolutionConfig) {
    let audits = Auditor::audit_agents(config);
    let counts = Auditor::count_files(config);
    let report = Auditor::generate_report(config, &audits, &counts);
    let analysis = Analyzer::analyze(&report);
    let proposals = Generator::propose_improvements(&report, &analysis, config);
    println!("{}", Generator::format_proposals(&proposals));

    if !config.auto_apply {
        println!("  (use --apply to write files, or --dry-run to preview)");
    }
}

fn cmd_validate(_config: &EvolutionConfig) {
    // Validate all existing agent files
    let agents_dir = EvolutionConfig::default().agents_dir;
    if !agents_dir.exists() {
        eprintln!("Agents directory not found: {}", agents_dir.display());
        return;
    }

    let walker = walkdir::WalkDir::new(&agents_dir).max_depth(1);
    let mut results = Vec::new();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "md") {
            continue;
        }
        let result = Validator::validate_file(path);
        let name = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
        results.push((name.to_string(), result));
    }

    println!("{}", Validator::format_validation(
        &results.iter().map(|(n, r)| (n.clone(), r.clone())).collect::<Vec<_>>()
    ));
}

fn cmd_evolve(config: EvolutionConfig, apply: bool, max_iter: Option<u64>, verbose: bool) {
    let result = evol_loop::run_evolution_cycle(
        Some(config.agents_dir.to_str().unwrap_or("/root/.agents/agents")),
        apply,
        max_iter,
        verbose,
    );

    match result {
        Ok(record) => {
            println!("\n{}", audit::format_audit_report(&record.audit));
            println!("{}", Analyzer::format_analysis(&record.analysis));

            if !record.improvements.is_empty() {
                println!("── Improvements Applied ──");
                for imp in &record.improvements {
                    let status = if imp.success { "✓" } else { "✗" };
                    println!("  [{}] {} {}", status, imp.proposal_id, imp.message);
                }
            }

            println!(
                "\nLoop completed: iteration {}, health {:.1}%, delta {:+.2}%, took {}ms",
                record.iteration,
                record.audit.ecosystem_health * 100.0,
                record.health_delta * 100.0,
                record.duration_ms
            );
        }
        Err(e) => {
            eprintln!("Evolution loop failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_status() {
    // Load persisted evolution state if available
    let config = EvolutionConfig::default();
    let output_dir = &config.output_dir;
    let persisted = SelfImprovementLoop::load_last_state(output_dir);

    // Run a fresh audit for current health
    let audits = Auditor::audit_agents(&config);
    let counts = Auditor::count_files(&config);
    let report = Auditor::generate_report(&config, &audits, &counts);

    println!("╔══════════════════════════════════════════╗");
    println!("║        Agent Ecosystem Status            ║");
    println!("╚══════════════════════════════════════════╝");
    println!("");
    println!("  Agents:      {}", report.agent_count);
    println!("  Commands:    {}", report.command_count);
    println!("  Skills:      {}", report.skill_count);
    println!("  Rules:       {}", report.rule_count);
    println!("  Health:      {:.1}%", report.ecosystem_health * 100.0);
    println!("");

    let high_issues: usize = report
        .audits
        .iter()
        .flat_map(|a| a.issues.iter())
        .filter(|i| matches!(i.severity, Severity::High | Severity::Critical))
        .count();
    let total_issues: usize = report.audits.iter().map(|a| a.issues.len()).sum();
    println!("  Issues:      {} total ({} high/critical)", total_issues, high_issues);

    let lang_coverage = report.language_coverage.len();
    println!("  Languages:   {}", lang_coverage);

    if let Some(state) = persisted {
        println!("");
        println!("  ── Evolution History ──");
        println!("  Iterations:  {}", state.iteration);
        if let Some(&last_health) = state.health_trajectory.last() {
            println!("  Last health: {:.1}%", last_health * 100.0);
        }
        if let Some(trend) = state.compute_health_trend() {
            let dir = if trend > 0.0 { "↑" } else { "↓" };
            println!("  Trend:       {} {:+.4}/iter", dir, trend);
        }
        println!("  Persisted:   {}", output_dir.join("evolution-state.txt").display());
    }

    match report.ecosystem_health {
        h if h >= 0.8 => println!("  Verdict:     Ecosystem is HEALTHY"),
        h if h >= 0.6 => println!("  Verdict:     Ecosystem is STABLE"),
        h if h >= 0.4 => println!("  Verdict:     Ecosystem needs IMPROVEMENT"),
        _ => println!("  Verdict:     Ecosystem is CRITICAL — run `aevolve evolve`"),
    }
    println!("────────────────────────────────────────");
}

fn cmd_full_report(config: &EvolutionConfig) {
    cmd_audit(config);
    cmd_analyze(config);
    cmd_propose(config);
}

fn cmd_meta(config: &EvolutionConfig) {
    // Run the full 7-pillar meta-evolution cycle
    let audits = Auditor::audit_agents(config);
    let counts = Auditor::count_files(config);
    let report = Auditor::generate_report(config, &audits, &counts);

    let mut meta = MetaEvolver::new();
    let meta_result = meta.cycle(&report, &audits, config);

    println!("{}", audit::format_audit_report(&report));
    println!("{}", meta_result.format());

    println!(
        "Meta cycle: {} to archive, {} self-play matches, {} explosions",
        meta_result.archive_additions,
        meta_result.self_play_matches,
        if meta_result.explosion_report.contains("DETECTED") { 1 } else { 0 }
    );
}

fn cmd_optimize(args: &[String]) {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        println!("{}", OPTIMIZER_USAGE);
        return;
    }

    match args[0].as_str() {
        "prompt" => cmd_optimize_prompt(&args[1..]),
        "compile" => cmd_optimize_compile(&args[1..]),
        _ => {
            eprintln!("Unknown optimize subcommand: {}", args[0]);
            println!("{}", OPTIMIZER_USAGE);
        }
    }
}

const OPTIMIZER_USAGE: &str = r#"
aevolve optimize — Low-Level Rust Code Optimization Loop

USAGE:
  aevolve optimize <subcommand> [args]

SUBCOMMANDS:
  prompt system               Print the System Prompt (LLM brain)
  prompt case-a <error>       Format Case A: compilation failure feedback
  prompt case-b <error>       Format Case B: logic/test failure feedback
  prompt case-c <time> <best> Format Case C: success feed-forward
  compile <code>              Compile Rust code and return result

EXAMPLES:
  aevolve optimize prompt system
  aevolve optimize prompt case-a "error[E0382]: borrow of moved value"
  aevolve optimize prompt case-b "thread panicked at 'assertion failed'"
  aevolve optimize prompt case-c 42.5 100.0
  aevolve optimize compile "pub fn add(a: i32, b: i32) -> i32 { a + b }"
"#;

fn cmd_optimize_prompt(args: &[String]) {
    if args.is_empty() {
        println!("{}", OPTIMIZER_USAGE);
        return;
    }

    match args[0].as_str() {
        "system" => {
            println!("{}", SYSTEM_PROMPT);
        }
        "case-a" => {
            let error = args[1..].join(" ");
            if error.is_empty() {
                eprintln!("Usage: aevolve optimize prompt case-a <compiler-error>");
                return;
            }
            println!("{}", OptimizationState::format_case_a(&error));
        }
        "case-b" => {
            let error = args[1..].join(" ");
            if error.is_empty() {
                eprintln!("Usage: aevolve optimize prompt case-b <test-error>");
                return;
            }
            println!("{}", OptimizationState::format_case_b(&error));
        }
        "case-c" => {
            if args.len() < 3 {
                eprintln!("Usage: aevolve optimize prompt case-c <time_ns> <best_time_ns>");
                return;
            }
            let time: f64 = args[1].parse().unwrap_or(0.0);
            let best: f64 = args[2].parse().unwrap_or(0.0);
            println!("{}", OptimizationState::format_case_c(time, best));
        }
        _ => {
            eprintln!("Unknown prompt type: {}", args[0]);
            println!("Available: system, case-a, case-b, case-c");
        }
    }
}

fn cmd_optimize_compile(args: &[String]) {
    let code = args.join(" ");
    if code.is_empty() {
        eprintln!("Usage: aevolve optimize compile <rust-code>");
        return;
    }

    let temp_dir = std::path::PathBuf::from("/tmp/aevolve_compile_test");
    let _ = std::fs::create_dir_all(&temp_dir);

    let result = Optimizer::compile(&code, "optimize_target", None, &temp_dir);

    if result.success {
        println!("COMPILE OK ({}ms)", result.duration_ms);
    } else {
        println!("COMPILE FAILED ({}ms)", result.duration_ms);
        println!("{}", result.error_log);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
