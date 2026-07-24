pub mod config;
pub mod core;
pub mod cortex;
pub mod dynamics;
pub mod evolution;
pub mod memory;
pub mod pipeline;
pub mod stability;
pub mod swarm;
pub mod units;

pub use config::*;
pub use core::*;
pub use cortex::Cortex;
pub use dynamics::cycle_step;
pub use evolution::{EvolutionTracker, SharedEvolutionTracker};
pub use memory::Memory;
pub use pipeline::{PipelineConfig, PipelineTransform, TransformationRegistry};
pub use stability::{
    create_labeled_test_data, create_test_data, run_sandbox_test, StabilityReport,
};
pub use swarm::{SwarmBus, SwarmMessage};
pub use units::{MutationReport, UnitHandle, UnitSnapshot};
