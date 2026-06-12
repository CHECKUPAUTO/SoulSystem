/// Mutation strategies available to the evolution engine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MutationStrategy {
    /// Replace `.clone()` calls with `&T` where possible.
    CloneToRef,
    /// Remove unnecessary `.into()` conversions.
    RedundantInto,
    /// Inline trivial wrapper functions.
    InlineWrapper,
    /// Replace manual loops with iterator adapters.
    LoopToIterator,
    /// Add missing `must_use` annotations.
    AddMustUse,
    /// Replace `unwrap` with `?` propagation.
    UnwrapToQuestion,
    /// Apply generic type simplifications.
    SimplifyType,
    /// Extract repeated literals into constants.
    ExtractConstant,
}
