use forge_bridge::binpack_demo::BinPacking;
use forge_bridge::{ForgeCampaign, ForgeConfig};
use forge_core::Candidate;

fn main() {
    for run in 0..3 {
        let cfg = ForgeConfig {
            generations: 5,
            population: 12,
            survivors: 2,
            base_seed: 0x1234_5678,
            max_reflection_retries: 0,
            stagnation_threshold: 100,
        };
        let r = ForgeCampaign::new(
            cfg,
            BinPacking {
                capacity: 1.0,
                n_items: 25,
                n_instances: 8,
            },
        )
        .run();
        println!("run {run}: history={:?}", r.history);
        if let Some(b) = &r.best {
            println!(
                "  best: obj={:?} cand={}",
                b.score.objectives,
                b.cand.repr()
            );
        }
    }
}
