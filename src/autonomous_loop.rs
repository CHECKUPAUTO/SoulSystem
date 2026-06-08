use crate::autonomous::AutonomousEntity;
use soul_automation::{AlertOperator, AlertSeverity};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

pub struct AutonomousLoopConfig {
    pub tick_interval_secs: u64,
    pub max_consecutive_noops: usize,
}

impl Default for AutonomousLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 30,
            max_consecutive_noops: 10,
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
}

pub async fn run_autonomous_loop(
    entity: &mut AutonomousEntity,
    config: AutonomousLoopConfig,
    shutdown: Arc<AtomicBool>,
) {
    info!("Autonomous loop starting (tick: {}s)", config.tick_interval_secs);

    setup_default_alerts(entity);

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

        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(config.tick_interval_secs)) => {}
            _ = shutdown_signal() => {
                info!("Autonomous loop: shutdown signal received");
                break;
            }
        }
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

    let context = format!(
        "Cycle {}. System: CPU={:.1}% MEM={:.1}% PROCS={}. Alerts: {}. Recent memory: {:?}",
        cycle,
        cpu,
        mem,
        procs,
        alerts.len(),
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
    let status = if result.action_taken { "ACTION" } else { "OBSERVE" };

    info!(
        "[Cycle {}] {} | Decision: {} | Alerts: {} | Obs: {}{}",
        result.cycle,
        status,
        result.decision,
        result.alerts.len(),
        result.observations.len(),
        if result.action_taken {
            format!(" | Result: {}", result.action_result.as_deref().unwrap_or("?"))
        } else {
            String::new()
        },
    );

    for alert in &result.alerts {
        warn!("[Cycle {}] ALERT: {}", result.cycle, alert);
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
