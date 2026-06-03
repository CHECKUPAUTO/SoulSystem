//! Smoke test du 9e bridge : `forge-bridge`.
//!
//! Comme `forge-bridge` n'expose pas (encore) de transport HTTP — c'est
//! le rôle du binaire `forge-service` à venir — on smoke-teste la couche
//! fonctionnelle : la campagne tourne, le `Report` est cohérent, les DTOs
//! sont JSON-clean. C'est exactement ce que les bridges en amont
//! (synergie, openevolve) consommeront.

use std::time::{Duration, Instant};

use forge_bridge::binpack_demo::BinPacking;
use forge_bridge::{CandidateDto, ForgeCampaign, ForgeConfig, ScoreDto};
use forge_core::Candidate;

#[test]
fn forge_bridge_smoke_full_campaign() {
    let start = Instant::now();
    let cfg = ForgeConfig {
        generations: 4,
        population: 12,
        survivors: 3,
        base_seed: 0xBADC_0FFE,
            max_reflection_retries: 2,
            stagnation_threshold: 5,
    };
    let domain = BinPacking { capacity: 1.0, n_items: 20, n_instances: 6 };
    let report = ForgeCampaign::new(cfg, domain).run();
    let elapsed = start.elapsed();

    println!(
        "[forge-bridge] gen={} pop={} survivors={} → history.len={}, best={:?}, holdout_best={:?}, elapsed={:?}",
        cfg.generations,
        cfg.population,
        cfg.survivors,
        report.history.len(),
        report.best.as_ref().map(|i| i.score.objectives.clone()),
        report.holdout_best.as_ref().map(|s| s.objectives.clone()),
        elapsed,
    );

    // Critères "fumée" :
    assert!(!report.history.is_empty(), "history doit être non-vide");
    assert!(report.best.is_some(), "best doit être trouvé");
    assert!(report.holdout_best.is_some(), "holdout doit être évalué");
    assert!(elapsed < Duration::from_secs(15), "campagne de smoke doit finir < 15s");
}

#[test]
fn forge_bridge_smoke_dto_json_clean() {
    // ScoreDto doit toujours produire un JSON parseable.
    let s = forge_core::Score::valid(vec![1.0, 2.0, 3.0]);
    let dto: ScoreDto = s.into();
    let json = serde_json::to_string(&dto).expect("ScoreDto → JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON → Value");
    assert_eq!(parsed["valid"], serde_json::json!(true));
    assert_eq!(parsed["objectives"], serde_json::json!([1.0, 2.0, 3.0]));

    // CandidateDto idem.
    let c = forge_bridge::binpack_demo::PackHeuristic { w: [0.25; 4] };
    let c_dto = CandidateDto::from(&c);
    let c_json = serde_json::to_string(&c_dto).expect("CandidateDto → JSON");
    let c_parsed: serde_json::Value =
        serde_json::from_str(&c_json).expect("JSON → Value");
    assert_eq!(
        c_parsed["id"].as_u64().unwrap(),
        forge_core::fnv1a(&c.repr()),
        "CandidateDto.id doit rester fnv1a(repr())"
    );
}
