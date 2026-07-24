use std::time::{Duration, Instant};
use tracing::instrument;

use crate::config::SandboxConfig;
use crate::{Data, StabilityError, Transformation};

#[derive(Debug, Clone)]
pub struct StabilityReport {
    pub divergence: f64,
    pub execution_time: Duration,
    pub output_size: usize,
    pub is_stable: bool,
}

#[instrument(skip(transformation))]
pub async fn run_sandbox_test(
    transformation: Box<dyn Transformation>,
    input: Vec<Data>,
    config: &SandboxConfig,
) -> Result<StabilityReport, StabilityError> {
    let start = Instant::now();
    let input_clone = input.clone();
    let timeout = config.timeout;

    // Auto-évaluation de stabilité déclarée par la transformation elle-même,
    // capturée avant de la déplacer dans la tâche bloquante. Elle est ensuite
    // combinée aux mesures empiriques pour former le verdict `is_stable`.
    let declared_stable = transformation.is_stable();

    let result = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || transformation.transform(&input_clone)),
    )
    .await;

    let output = match result {
        Ok(Ok(Ok(out))) => out,
        Ok(Ok(Err(e))) => return Err(StabilityError::TransformationError(e)),
        Ok(Err(_join)) => return Err(StabilityError::InfiniteLoop),
        Err(_elapsed) => return Err(StabilityError::InfiniteLoop),
    };

    let execution_time = start.elapsed();

    let output_size: usize = output.iter().map(|d| d.payload.len()).sum();
    if output_size > config.max_output_bytes {
        return Err(StabilityError::MemoryOverflow(output_size));
    }

    let divergence = calculate_divergence(&input, &output);
    if divergence > config.max_divergence {
        return Err(StabilityError::ExcessiveDivergence(divergence));
    }

    // Verdict réel : la transformation doit se déclarer stable ET rester sous le
    // seuil de divergence. Le champ n'est plus une constante — il conditionne
    // l'adoption d'une mutation côté acteur (cf. units::handle_mutation).
    let is_stable = declared_stable && divergence <= config.max_divergence;

    Ok(StabilityReport {
        divergence,
        execution_time,
        output_size,
        is_stable,
    })
}

fn calculate_divergence(input: &[Data], output: &[Data]) -> f64 {
    if input.is_empty() && output.is_empty() {
        return 0.0;
    }
    if input.is_empty() || output.is_empty() {
        return 100.0;
    }

    let total_in: usize = input.iter().map(|d| d.payload.len()).sum();
    let total_out: usize = output.iter().map(|d| d.payload.len()).sum();

    let ratio = if total_in == 0 {
        0.0
    } else {
        total_out as f64 / total_in as f64
    };
    let size_divergence = (ratio - 1.0).abs() * 100.0;

    let min_len = input.len().min(output.len());
    let mut byte_diff = 0usize;
    for i in 0..min_len {
        let in_bytes = &input[i].payload;
        let out_bytes = &output[i].payload;
        let min_b = in_bytes.len().min(out_bytes.len());
        for j in 0..min_b {
            if in_bytes[j] != out_bytes[j] {
                byte_diff += 1;
            }
        }
        byte_diff += in_bytes.len().abs_diff(out_bytes.len());
    }

    let max_len = input[0].payload.len().max(output[0].payload.len());
    let byte_rate = if min_len == 0 || max_len == 0 {
        0.0
    } else {
        byte_diff as f64 / (min_len * max_len) as f64
    };

    size_divergence * 0.7 + byte_rate * 100.0 * 0.3
}

pub fn create_test_data(size: usize, count: usize) -> Vec<Data> {
    (0..count)
        .map(|i| {
            let payload: Vec<u8> = (0..size).map(|j| ((i * size + j) % 256) as u8).collect();
            Data::new(payload)
        })
        .collect()
}

pub fn create_labeled_test_data(size: usize, count: usize, label: &str) -> Vec<Data> {
    (0..count)
        .map(|i| {
            let payload: Vec<u8> = (0..size).map(|j| ((i * size + j) % 256) as u8).collect();
            Data::new(payload).tag(label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SandboxConfig;
    use crate::{AmplifyTransform, IdentityTransform};

    fn quick_config() -> SandboxConfig {
        SandboxConfig {
            timeout: Duration::from_millis(100),
            max_output_bytes: 10_240,
            max_divergence: 100.0,
        }
    }

    #[tokio::test]
    async fn test_stable_transform() {
        let config = quick_config();
        let input = create_test_data(4, 3);
        let result = run_sandbox_test(Box::new(IdentityTransform), input, &config).await;
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.is_stable);
    }

    #[tokio::test]
    async fn test_memory_overflow() {
        let config = SandboxConfig {
            max_output_bytes: 16,
            ..quick_config()
        };
        let input = create_test_data(4, 1);
        let result =
            run_sandbox_test(Box::new(AmplifyTransform { factor: 10 }), input, &config).await;
        assert!(matches!(result, Err(StabilityError::MemoryOverflow(_))));
    }

    #[test]
    fn test_divergence_identical() {
        let input = create_test_data(4, 2);
        let output = input.clone();
        let div = calculate_divergence(&input, &output);
        assert_eq!(div, 0.0);
    }

    #[test]
    fn test_divergence_empty() {
        assert_eq!(calculate_divergence(&[], &[]), 0.0);
    }

    #[test]
    fn test_divergence_one_empty() {
        let data = create_test_data(4, 2);
        assert!(calculate_divergence(&data, &[]) > 0.0);
    }

    #[test]
    fn test_create_test_data_uniqueness() {
        let data = create_test_data(8, 5);
        assert_eq!(data.len(), 5);
        assert_eq!(data[0].payload.len(), 8);
        assert_ne!(data[0].payload, data[1].payload);
    }

    #[test]
    fn test_labeled_test_data() {
        let data = create_labeled_test_data(4, 3, "test-label");
        assert_eq!(data.len(), 3);
        assert!(data[0].metadata.tags.contains(&"test-label".to_string()));
    }
}
