pub mod analyzer;
pub mod audit;
pub mod evol_loop;
pub mod frontmatter;
pub mod generator;
pub mod loader;
pub mod meta_evolution;
pub mod optimizer;
pub mod registry;
pub mod types;
pub mod validator;

pub use evol_loop::{run_evolution_cycle, SelfImprovementLoop};
pub use loader::DynamicModuleLoader;
pub use meta_evolution::{
    format_explosion_report, GodelEngine, MetaCycleResult, MetaEvolver, ReflexionMemory,
    SelfPlayArena,
};
pub use optimizer::{
    BenchResult, CompileResult, OptimizationAttempt, OptimizationState, Optimizer, TestResult,
    CASE_A_COMPILE_FAILURE, CASE_B_LOGIC_FAILURE, CASE_C_SUCCESS, SYSTEM_PROMPT,
};
pub use registry::{
    agent_name_to_id, load_and_register, register_agents, scan_agents, AgentRegistryEntry,
};
pub use types::EvolutionConfig;
pub use types::{
    AgentArchive, ArchiveEntry, ExplosionMetrics, GodelStrategy, Improver, ImproverStrategy,
    OptimizationTrajectory, ReflexionEpisode, SelfModProposal, SelfPlayMatch, TrajectoryPoint,
    UtilityFunction,
};
