use std::fmt;

use crate::core::data::Data;
use crate::core::error::TransformationError;
use crate::evolution::EvolutionTracker;

pub trait Transformation: Send + Sync + fmt::Debug {
    fn name(&self) -> &str;

    fn transform(&self, input: &[Data]) -> Result<Vec<Data>, TransformationError>;

    fn is_stable(&self) -> bool;

    fn clone_box(&self) -> Box<dyn Transformation>;

    fn describe(&self) -> String {
        format!("{} (stable: {})", self.name(), self.is_stable())
    }

    /// Configuration sérialisable de l'algorithme, compatible avec la
    /// [`crate::pipeline::TransformationRegistry`]. Permet à une unité de
    /// **partager** son algorithme courant (nom + config) sur le bus swarm pour
    /// que ses pairs puissent le **reconstruire et l'adopter** (transfert
    /// horizontal). Par défaut, aucune configuration paramétrique.
    fn config_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

impl Clone for Box<dyn Transformation> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Debug, Clone)]
pub struct IdentityTransform;

impl Transformation for IdentityTransform {
    fn name(&self) -> &str {
        "identity"
    }

    fn transform(&self, input: &[Data]) -> Result<Vec<Data>, TransformationError> {
        Ok(input.to_vec())
    }

    fn is_stable(&self) -> bool {
        true
    }

    fn clone_box(&self) -> Box<dyn Transformation> {
        Box::new(IdentityTransform)
    }
}

#[derive(Debug, Clone)]
pub struct ReverseTransform;

impl Transformation for ReverseTransform {
    fn name(&self) -> &str {
        "reverse"
    }

    fn transform(&self, input: &[Data]) -> Result<Vec<Data>, TransformationError> {
        let mut output: Vec<Data> = input.to_vec();
        output.reverse();
        Ok(output)
    }

    fn is_stable(&self) -> bool {
        true
    }

    fn clone_box(&self) -> Box<dyn Transformation> {
        Box::new(ReverseTransform)
    }
}

#[derive(Debug, Clone)]
pub struct AmplifyTransform {
    pub factor: usize,
}

impl Transformation for AmplifyTransform {
    fn name(&self) -> &str {
        "amplify"
    }

    fn transform(&self, input: &[Data]) -> Result<Vec<Data>, TransformationError> {
        let cap = input.len().saturating_mul(self.factor);
        let mut output = Vec::with_capacity(cap);
        for data in input {
            for _ in 0..self.factor {
                output.push(data.clone());
            }
        }
        Ok(output)
    }

    fn is_stable(&self) -> bool {
        self.factor <= 3
    }

    fn clone_box(&self) -> Box<dyn Transformation> {
        Box::new(self.clone())
    }

    fn config_json(&self) -> serde_json::Value {
        serde_json::json!({ "factor": self.factor })
    }
}

#[derive(Debug, Clone)]
pub struct HangingTransform;

impl Transformation for HangingTransform {
    fn name(&self) -> &str {
        "hanging"
    }

    fn transform(&self, _input: &[Data]) -> Result<Vec<Data>, TransformationError> {
        let start = std::time::Instant::now();
        while start.elapsed().as_millis() < 200 {
            std::hint::spin_loop();
        }
        Ok(Vec::new())
    }

    fn is_stable(&self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn Transformation> {
        Box::new(HangingTransform)
    }
}

pub fn generate_mutation(
    current: &dyn Transformation,
    tracker: &EvolutionTracker,
) -> Box<dyn Transformation> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    if !tracker.should_explore() {
        if let Some((best_family, _fitness)) = tracker.best_family() {
            let result = match best_family.as_str() {
                "reverse" => Some(Box::new(ReverseTransform) as Box<dyn Transformation>),
                "amplify" => Some(Box::new(AmplifyTransform {
                    factor: rng.gen_range(1..6),
                }) as Box<dyn Transformation>),
                "identity" => Some(Box::new(IdentityTransform) as Box<dyn Transformation>),
                _ => None,
            };
            if let Some(algo) = result {
                return algo;
            }
        }
    }

    let variety: u8 = match current.name() {
        "hanging" => rng.gen_range(0..3),
        _ => rng.gen_range(0..5),
    };

    match variety {
        0 => Box::new(IdentityTransform),
        1 => Box::new(ReverseTransform),
        2 => Box::new(AmplifyTransform {
            factor: rng.gen_range(1..6),
        }),
        3 => Box::new(AmplifyTransform {
            factor: rng.gen_range(15..50),
        }),
        4 => Box::new(HangingTransform),
        _ => Box::new(IdentityTransform),
    }
}
