pub mod loader;
pub mod types;
pub mod frontmatter;
pub mod audit;
pub mod analyzer;
pub mod generator;
pub mod validator;
pub mod evol_loop;
pub mod optimizer;
pub mod meta_evolution;
pub mod registry;

pub use loader::DynamicModuleLoader;
pub use evol_loop::{run_evolution_cycle, SelfImprovementLoop};
pub use types::EvolutionConfig;
pub use optimizer::{
    Optimizer, OptimizationState, OptimizationAttempt,
    CompileResult, TestResult, BenchResult,
    SYSTEM_PROMPT, CASE_A_COMPILE_FAILURE, CASE_B_LOGIC_FAILURE, CASE_C_SUCCESS,
};
pub use meta_evolution::{
    GodelEngine, MetaEvolver, MetaCycleResult,
    SelfPlayArena, ReflexionMemory, format_explosion_report,
};
pub use types::{
    GodelStrategy, SelfModProposal,
    AgentArchive, ArchiveEntry,
    Improver, ImproverStrategy,
    SelfPlayMatch, ReflexionEpisode,
    OptimizationTrajectory, TrajectoryPoint,
    ExplosionMetrics, UtilityFunction,
};
pub use registry::{scan_agents, load_and_register, register_agents, agent_name_to_id, AgentRegistryEntry};
