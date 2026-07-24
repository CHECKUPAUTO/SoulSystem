use crate::{Data, Transformation, TransformationError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub stages: Vec<PipelineStageConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStageConfig {
    pub name: String,
    pub config: serde_json::Value,
}

impl PipelineConfig {
    pub fn empty() -> Self {
        PipelineConfig { stages: Vec::new() }
    }

    pub fn single(name: &str) -> Self {
        PipelineConfig {
            stages: vec![PipelineStageConfig {
                name: name.to_string(),
                config: serde_json::json!({}),
            }],
        }
    }

    pub fn add_stage(mut self, name: &str, config: serde_json::Value) -> Self {
        self.stages.push(PipelineStageConfig {
            name: name.to_string(),
            config,
        });
        self
    }
}

#[derive(Debug, Clone)]
pub struct PipelineTransform {
    name: String,
    stages: Vec<Box<dyn Transformation>>,
    stage_names: Vec<String>,
}

impl PipelineTransform {
    pub fn new(stages: Vec<Box<dyn Transformation>>) -> Self {
        let stage_names: Vec<String> = stages.iter().map(|s| s.name().to_string()).collect();
        let name = format!("pipeline[{}]", stage_names.join("->"));
        PipelineTransform {
            name,
            stages,
            stage_names,
        }
    }

    pub fn from_config(
        config: &PipelineConfig,
        registry: &TransformationRegistry,
    ) -> Result<Self, PipelineError> {
        let mut stages = Vec::new();
        let mut stage_names = Vec::new();
        for stage_config in &config.stages {
            let stage = registry.create(&stage_config.name, &stage_config.config)?;
            stage_names.push(stage_config.name.clone());
            stages.push(stage);
        }
        let name = format!("pipeline[{}]", stage_names.join("->"));
        Ok(PipelineTransform {
            name,
            stages,
            stage_names,
        })
    }

    pub fn stages(&self) -> &[Box<dyn Transformation>] {
        &self.stages
    }

    pub fn stage_names(&self) -> &[String] {
        &self.stage_names
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }
}

impl Transformation for PipelineTransform {
    fn name(&self) -> &str {
        &self.name
    }

    fn transform(&self, input: &[Data]) -> Result<Vec<Data>, TransformationError> {
        let mut data = input.to_vec();
        for stage in &self.stages {
            data = stage.transform(&data)?;
        }
        Ok(data)
    }

    fn is_stable(&self) -> bool {
        self.stages.iter().all(|s| s.is_stable())
    }

    fn clone_box(&self) -> Box<dyn Transformation> {
        Box::new(self.clone())
    }

    fn describe(&self) -> String {
        format!("{} ({} stages)", self.name, self.stages.len())
    }
}

pub struct TransformationRegistry {
    creators: HashMap<
        String,
        Box<
            dyn Fn(&serde_json::Value) -> Result<Box<dyn Transformation>, PipelineError>
                + Send
                + Sync,
        >,
    >,
}

impl TransformationRegistry {
    pub fn new() -> Self {
        TransformationRegistry {
            creators: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &str, creator: F)
    where
        F: Fn(&serde_json::Value) -> Result<Box<dyn Transformation>, PipelineError>
            + Send
            + Sync
            + 'static,
    {
        self.creators.insert(name.to_string(), Box::new(creator));
    }

    pub fn create(
        &self,
        name: &str,
        config: &serde_json::Value,
    ) -> Result<Box<dyn Transformation>, PipelineError> {
        if let Some(creator) = self.creators.get(name) {
            creator(config)
        } else {
            Err(PipelineError::UnknownTransformation(name.to_string()))
        }
    }
}

impl Default for TransformationRegistry {
    fn default() -> Self {
        let mut registry = TransformationRegistry::new();
        registry.register("identity", |_| Ok(Box::new(crate::IdentityTransform)));
        registry.register("reverse", |_| Ok(Box::new(crate::ReverseTransform)));
        registry.register("amplify", |config| {
            let factor = config.get("factor").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            Ok(Box::new(crate::AmplifyTransform { factor }))
        });
        registry.register("hanging", |_| Ok(Box::new(crate::HangingTransform)));
        registry
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Unknown transformation: {0}")]
    UnknownTransformation(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Transformation error: {0}")]
    TransformationError(#[from] TransformationError),
}

pub fn make_identity_reverse_pipeline() -> PipelineTransform {
    PipelineTransform::new(vec![
        Box::new(crate::IdentityTransform),
        Box::new(crate::ReverseTransform),
    ])
}

pub fn make_reverse_amplify_pipeline(factor: usize) -> PipelineTransform {
    PipelineTransform::new(vec![
        Box::new(crate::ReverseTransform),
        Box::new(crate::AmplifyTransform { factor }),
    ])
}

#[cfg(test)]
mod tests {
    use crate::{AmplifyTransform, Data, IdentityTransform, ReverseTransform, Transformation};

    #[test]
    fn test_pipeline_name() {
        let pipe = super::PipelineTransform::new(vec![
            Box::new(IdentityTransform),
            Box::new(ReverseTransform),
        ]);
        assert_eq!(pipe.name(), "pipeline[identity->reverse]");
    }

    #[test]
    fn test_pipeline_transform() {
        let pipe = super::PipelineTransform::new(vec![
            Box::new(IdentityTransform),
            Box::new(ReverseTransform),
        ]);
        let input = vec![Data::new(vec![1, 2]), Data::new(vec![3, 4])];
        let output = pipe.transform(&input).unwrap();
        assert_eq!(output[0].payload, vec![3, 4]);
        assert_eq!(output[1].payload, vec![1, 2]);
    }

    #[test]
    fn test_pipeline_stability() {
        let pipe = super::PipelineTransform::new(vec![
            Box::new(crate::IdentityTransform),
            Box::new(AmplifyTransform { factor: 2 }),
        ]);
        assert!(pipe.is_stable());
    }

    #[test]
    fn test_pipeline_unstable() {
        let pipe = super::PipelineTransform::new(vec![
            Box::new(crate::IdentityTransform),
            Box::new(crate::AmplifyTransform { factor: 10 }),
            Box::new(crate::HangingTransform),
        ]);
        assert!(!pipe.is_stable());
    }

    #[test]
    fn test_pipeline_empty() {
        let pipe = super::PipelineTransform::new(vec![]);
        let input = vec![Data::new(vec![1, 2, 3])];
        let output = pipe.transform(&input).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn test_pipeline_stages() {
        let pipe = super::PipelineTransform::new(vec![
            Box::new(crate::IdentityTransform),
            Box::new(crate::ReverseTransform),
        ]);
        assert_eq!(pipe.stage_count(), 2);
        assert_eq!(
            pipe.stage_names(),
            &["identity".to_string(), "reverse".to_string()]
        );
    }

    #[test]
    fn test_registry_default() {
        let registry = super::TransformationRegistry::default();
        let identity = registry.create("identity", &serde_json::json!({}));
        assert!(identity.is_ok());
        assert_eq!(identity.unwrap().name(), "identity");
    }

    #[test]
    fn test_registry_unknown() {
        let registry = super::TransformationRegistry::default();
        let result = registry.create("foo", &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_amplify_with_config() {
        let registry = super::TransformationRegistry::default();
        let amplify = registry
            .create("amplify", &serde_json::json!({"factor": 5}))
            .unwrap();
        assert_eq!(amplify.name(), "amplify");
    }

    #[test]
    fn test_pipeline_from_config() {
        let config = super::PipelineConfig::single("identity");
        assert_eq!(config.stages.len(), 1);
    }

    #[test]
    fn test_pipeline_config_empty() {
        let config = super::PipelineConfig::empty();
        assert!(config.stages.is_empty());
    }

    #[test]
    fn test_make_identity_reverse_pipeline() {
        let pipe = super::make_identity_reverse_pipeline();
        assert!(pipe.name().contains("identity"));
        assert!(pipe.name().contains("reverse"));
    }

    #[test]
    fn test_make_reverse_amplify_pipeline() {
        let pipe = super::make_reverse_amplify_pipeline(3);
        assert!(pipe.name().contains("reverse"));
        assert!(pipe.name().contains("amplify"));
    }
}
