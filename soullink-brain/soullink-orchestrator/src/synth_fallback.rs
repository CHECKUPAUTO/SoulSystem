//! Fallback reply synthesis — used when Ollama is unreachable or
//! disabled. Produces a plain-text summary of the mesh state without
//! any external dependency.
//!
//! The user-visible impact: instead of seeing nothing or an error, the
//! user gets a mesh-telemetry snapshot with a clear note that the full
//! language response is temporarily unavailable.

use crate::mesh_context::MeshContext;

pub fn build(ctx: &MeshContext, reason: Option<&str>) -> String {
    let note = match reason {
        Some(r) => format!("\n\n(⚠️  Language model unavailable — {r}. Mesh telemetry only.)"),
        None => String::new(),
    };

    if ctx.top_concepts.is_empty() {
        return format!(
            "Mesh consulted ({} brain(s)). No concepts surfaced for \"{}\". \
             Dominant attractor: {}. Turbulence avg={:.2}.{}",
            ctx.brains.len(),
            ctx.query,
            ctx.attractor_dominant,
            ctx.turbulence_avg,
            note,
        );
    }

    let concepts: Vec<String> = ctx
        .top_concepts
        .iter()
        .take(5)
        .map(|c| {
            format!(
                "• {} (score {:.2}, from {})",
                c.concept, c.score, c.source_brain
            )
        })
        .collect();

    format!(
        "Mesh snapshot for \"{}\":\n\
         \n\
         Consulted {} brain(s). Dominant attractor: {}. Turbulence avg={:.2}, max={:.2}.\n\
         \n\
         Top concepts:\n{}{}",
        ctx.query,
        ctx.brains.len(),
        ctx.attractor_dominant,
        ctx.turbulence_avg,
        ctx.turbulence_max,
        concepts.join("\n"),
        note,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_context::MeshContext;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn fallback_with_no_concepts() {
        let results = HashMap::from([(
            "meta".to_string(),
            json!({"attractor": "Transient", "turbulence": 0.4}),
        )]);
        let ctx = MeshContext::build("hi", &results, &[]);
        let text = build(&ctx, Some("timeout"));
        assert!(text.contains("No concepts surfaced"));
        assert!(text.contains("Transient"));
        assert!(text.contains("timeout"));
    }

    #[test]
    fn fallback_with_concepts() {
        let results = HashMap::from([(
            "science".to_string(),
            json!({"attractor": "DeepBasin", "turbulence": 0.2}),
        )]);
        let merged = vec![
            json!({"concept": "entropy",    "score": 0.9, "source_brain": "science"}),
            json!({"concept": "relativity", "score": 0.7, "source_brain": "science"}),
        ];
        let ctx = MeshContext::build("physics", &results, &merged);
        let text = build(&ctx, None);
        assert!(text.contains("entropy"));
        assert!(text.contains("relativity"));
        assert!(!text.contains("unavailable"), "no reason → no warning tag");
    }
}
