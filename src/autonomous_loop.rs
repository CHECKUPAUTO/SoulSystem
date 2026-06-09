use crate::autonomous::AutonomousEntity;
use soul_automation::{AlertOperator, AlertSeverity};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

pub struct AutonomousLoopConfig {
    pub tick_interval_secs: u64,
    pub max_consecutive_noops: usize,
    pub data_dir: String,
    pub dashboard_port: Option<u16>,
}

impl Default for AutonomousLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 30,
            max_consecutive_noops: 10,
            data_dir: "/var/lib/soulsystem/autonomous".to_string(),
            dashboard_port: None,
        }
    }
}

pub struct CycleResult {
    pub cycle: usize,
    pub observations: Vec<String>,
    pub alerts: Vec<String>,
    pub decision: String,
    pub action_taken: bool,
    pub action_result: Option<String>,
    pub cron_results: Vec<(String, bool, String)>,
    pub workflow_results: Vec<String>,
    pub healings: Vec<String>,
}

pub async fn run_autonomous_loop(
    entity: &mut AutonomousEntity,
    config: AutonomousLoopConfig,
    shutdown: Arc<AtomicBool>,
) {
    info!("Autonomous loop starting (tick: {}s)", config.tick_interval_secs);

    setup_default_alerts(entity);
    entity.register_healing_actions();

    let _ = entity.load_state(&config.data_dir);

    if let Some(port) = config.dashboard_port {
        let data_dir = config.data_dir.clone();
        tokio::spawn(async move {
            serve_dashboard(port, data_dir).await;
        });
        info!("Dashboard serving on :{}", port);
    }

    let mut cycle_count: usize = 0;
    let mut consecutive_noops: usize = 0;

    while !shutdown.load(Ordering::Relaxed) {
        cycle_count += 1;
        let result = execute_cycle(entity, cycle_count).await;

        log_cycle_result(&result);

        if !result.action_taken {
            consecutive_noops += 1;
        } else {
            consecutive_noops = 0;
        }

        if consecutive_noops >= config.max_consecutive_noops {
            warn!(
                "Autonomous loop: {} consecutive no-op cycles, adjusting behavior",
                consecutive_noops
            );
            consecutive_noops = 0;
        }

        if cycle_count.is_multiple_of(10) {
            if let Err(e) = entity.save_state(&config.data_dir) {
                warn!("Failed to save state: {}", e);
            } else {
                info!("State saved (cycle {})", cycle_count);
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(config.tick_interval_secs)) => {}
            _ = shutdown_signal() => {
                info!("Autonomous loop: shutdown signal received");
                break;
            }
        }
    }

    if let Err(e) = entity.save_state(&config.data_dir) {
        warn!("Final state save failed: {}", e);
    } else {
        info!("Final state saved");
    }

    info!("Autonomous loop stopped after {} cycles", cycle_count);
}

async fn execute_cycle(entity: &mut AutonomousEntity, cycle: usize) -> CycleResult {
    let mut observations = Vec::new();

    let metrics = soul_bridges::monitor::get_metrics();
    let cpu = metrics.cpu_usage;
    let mem = metrics.memory_usage;
    let procs = metrics.process_count;

    observations.push(format!("CPU: {:.1}%, RAM: {:.1}%, Processes: {}", cpu, mem, procs));

    if let Some(gpu) = entity.monitor.gpu.get_gpu_info() {
        observations.push(format!(
            "GPU: {}°C, {:.0}% util, {}MB/{}MB VRAM",
            gpu.temperature, gpu.utilization, gpu.memory_used, gpu.memory_total
        ));
        entity.monitor.predictive.record("gpu_temp", gpu.temperature as f64);
    }

    entity.monitor.predictive.record("cpu", cpu as f64);
    entity.monitor.predictive.record("memory", mem as f64);

    let disks = entity.monitor.disk.get_disks();
    for d in &disks {
        if d.usage_percent > 80.0 {
            observations.push(format!("DISK WARNING: {} at {:.0}%", d.mount_point, d.usage_percent));
        }
    }

    let alerts = entity.check_alerts();

    let cron_results = entity.tick_cron();
    for (task_id, success, output) in &cron_results {
        observations.push(format!("Cron {}: {} - {}", task_id, if *success { "OK" } else { "FAIL" }, output.chars().take(50).collect::<String>()));
    }

    let workflow_results = entity.tick_workflows();
    for wr in &workflow_results {
        observations.push(format!("Workflow: {}", wr));
    }

    let healings = entity.diagnose_and_heal();
    for h in &healings {
        observations.push(format!("Heal: {}", h));
    }

    let context = format!(
        "Cycle {}. System: CPU={:.1}% MEM={:.1}% PROCS={}. Alerts: {}. Cron: {}. Workflows: {}. Healings: {}. Recent: {:?}",
        cycle,
        cpu,
        mem,
        procs,
        alerts.len(),
        cron_results.len(),
        workflow_results.len(),
        healings.len(),
        entity.planner.memory.recent_observations(3),
    );

    let decision = entity.decide(&context);
    let decision_str = format!("{} (conf: {:.2})", decision.action, decision.confidence);

    let mut action_taken = false;
    let mut action_result = None;

    if decision.confidence > 0.5 && decision.action != "monitor" && !decision.action.is_empty() {
        let goal = entity.create_goal(&decision.action);
        let plan = entity.plan(&goal);

        if !plan.steps.is_empty() {
            match entity.execute_plan(&plan) {
                Ok(result) => {
                    action_taken = true;
                    action_result = Some(result.clone());
                    entity.learn(&decision.action, &result, 0.8);
                    observations.push(format!("Action executed: {}", decision.action));
                }
                Err(e) => {
                    entity.learn(&decision.action, &e, -0.5);
                    observations.push(format!("Action failed: {} - {}", decision.action, e));
                }
            }
        }
    }

    for obs in &observations {
        entity.observe(obs);
    }

    CycleResult {
        cycle,
        observations,
        alerts,
        decision: decision_str,
        action_taken,
        action_result,
        cron_results,
        workflow_results,
        healings,
    }
}

fn setup_default_alerts(entity: &mut AutonomousEntity) {
    entity.automation.alerts.add_rule(
        "high_cpu",
        "cpu",
        80.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    entity.automation.alerts.add_rule(
        "critical_cpu",
        "cpu",
        95.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Critical,
    );
    entity.automation.alerts.add_rule(
        "high_memory",
        "memory",
        85.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    entity.automation.alerts.add_rule(
        "critical_memory",
        "memory",
        95.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Critical,
    );
    entity.automation.alerts.add_rule(
        "high_gpu_temp",
        "gpu_temp",
        75.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    entity.automation.alerts.add_rule(
        "critical_gpu_temp",
        "gpu_temp",
        85.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Critical,
    );

    info!("Default alert rules configured: CPU>80%, RAM>85%, GPU>75°C");
}

fn log_cycle_result(result: &CycleResult) {
    let mut parts = vec![
        format!("[Cycle {}]", result.cycle),
        if result.action_taken { "ACTION".to_string() } else { "OBSERVE".to_string() },
        format!("Decision: {}", result.decision),
    ];

    if !result.cron_results.is_empty() {
        parts.push(format!("Cron: {}", result.cron_results.len()));
    }
    if !result.workflow_results.is_empty() {
        parts.push(format!("WF: {}", result.workflow_results.len()));
    }
    if !result.healings.is_empty() {
        parts.push(format!("Heal: {}", result.healings.len()));
    }

    info!("{}", parts.join(" | "));

    for alert in &result.alerts {
        warn!("[Cycle {}] ALERT: {}", result.cycle, alert);
    }
}

async fn serve_dashboard(port: u16, _data_dir: String) {
    use axum::routing::get;
    use axum::Router;

    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>SoulSystem Autonomous Dashboard</title>
    <meta charset="utf-8">
    <meta http-equiv="refresh" content="10">
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #1a1a2e; color: #eee; }
        .container { max-width: 1200px; margin: 0 auto; }
        .card { background: #16213e; border-radius: 8px; padding: 20px; margin: 10px 0; }
        .metric { display: inline-block; margin: 10px; padding: 15px; background: #0f3460; border-radius: 8px; min-width: 150px; }
        .metric h3 { margin: 0; color: #e94560; }
        .metric p { margin: 5px 0 0; font-size: 24px; }
        h1 { color: #e94560; }
        .online { color: #4caf50; }
        .offline { color: #f44336; }
        pre { white-space: pre-wrap; word-break: break-all; }
    </style>
</head>
<body>
    <div class="container">
        <h1>SoulSystem Autonomous Dashboard</h1>
        <div class="card">
            <h2>Status</h2>
            <pre id="status">Loading...</pre>
        </div>
        <div class="card">
            <h2>Recent Activity</h2>
            <pre id="activity">Loading...</pre>
        </div>
    </div>
    <script>
        fetch('/api/status').then(r => r.json()).then(d => {
            document.getElementById('status').textContent = JSON.stringify(d, null, 2);
        });
        fetch('/api/activity').then(r => r.json()).then(d => {
            document.getElementById('activity').textContent = JSON.stringify(d, null, 2);
        });
    </script>
</body>
</html>"#.to_string();

    let app = Router::new()
        .route("/", get(move || {
            let html = html.clone();
            async move { axum::response::Html(html) }
        }))
        .route("/api/status", get(|| async {
            axum::Json(serde_json::json!({"status": "running"}))
        }))
        .route("/api/activity", get(|| async {
            axum::Json(serde_json::json!({"activity": []}))
        }));

    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => {
            warn!("Dashboard: failed to bind port {}: {}", port, e);
            return;
        }
    };

    info!("Dashboard listening on :{}", port);
    if let Err(e) = axum::serve(listener, app).await {
        warn!("Dashboard error: {}", e);
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
