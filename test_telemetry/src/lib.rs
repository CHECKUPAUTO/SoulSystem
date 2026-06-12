#[cfg(test)]
mod tests {
    use soul_telemetry::{TelemetryHub, PrometheusExporter, gather_metrics};

    #[test]
    fn telemetry_hub_integration() {
        let hub = TelemetryHub::new(4);

        hub.record_execution(0, 100, false);
        hub.record_execution(1, 200, true);
        hub.record_execution(0, 150, false);

        let (total_tasks, total_cycles) = hub.aggregate_metrics();
        assert_eq!(total_tasks, 3);
        assert_eq!(total_cycles, 450);

        let core_metrics = hub.get_core_metrics();
        assert_eq!(core_metrics.len(), 4);
        assert_eq!(core_metrics[0].tasks_executed, 2);
        assert_eq!(core_metrics[1].tasks_executed, 1);
        assert_eq!(core_metrics[1].tasks_stolen, 1);
    }

    #[test]
    fn telemetry_hub_exporter_integration() {
        let hub = TelemetryHub::new(8);

        for i in 0..8 {
            hub.record_execution(i, 100 * (i as u64 + 1), i % 2 == 0);
        }

        let exporter = PrometheusExporter::new().expect("Failed to create exporter");
        exporter.update_from_hub(&hub);

        let output = gather_metrics().expect("Failed to gather metrics");
        assert!(output.contains("soul_scheduler_cycles_total"));
    }

    #[test]
    fn thermal_status_integration() {
        let hub = TelemetryHub::new(2);

        assert!(!hub.check_thermal_status(0));
        assert_eq!(hub.current_temp_celsius(), 0);
    }

    #[test]
    fn core_metrics_snapshot_integration() {
        let hub = TelemetryHub::new(4);

        hub.record_execution(0, 50, true);
        hub.record_execution(1, 50, false);
        hub.record_execution(1, 50, false);
        hub.record_execution(2, 50, true);
        hub.record_execution(2, 50, true);
        hub.record_execution(2, 50, true);
        hub.record_execution(3, 50, false);
        hub.record_execution(3, 50, false);
        hub.record_execution(3, 50, false);
        hub.record_execution(3, 50, false);

        let snapshots = hub.get_core_metrics();

        assert_eq!(snapshots[0].tasks_executed, 1);
        assert_eq!(snapshots[0].tasks_stolen, 1);
        assert_eq!(snapshots[0].total_cycles, 50);

        assert_eq!(snapshots[1].tasks_executed, 2);
        assert_eq!(snapshots[1].tasks_stolen, 0);

        assert_eq!(snapshots[2].tasks_executed, 3);
        assert_eq!(snapshots[2].tasks_stolen, 3);

        assert_eq!(snapshots[3].tasks_executed, 4);
        assert_eq!(snapshots[3].tasks_stolen, 0);
    }
}
