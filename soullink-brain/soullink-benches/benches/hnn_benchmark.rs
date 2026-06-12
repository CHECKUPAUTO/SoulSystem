use criterion::{black_box, criterion_group, criterion_main, Criterion};

// NOTE: Le benchmark utilise une simulation simplifiée de tick HNN.
// Quand soullink-autonomy sera disponible comme dépendance, remplacer
// cette simulation par l'API réelle :
//
//   use soullink_autonomy::pulse::AutonomyPulse;
//   use soullink_autonomy::afferent::AfferentNerve;
//   use std::sync::Arc;
//
//   let afferent = Arc::new(AfferentNerve::mock());
//   let pulse = AutonomyPulse::new(afferent);
//   b.iter(|| {
//       for _ in 0..10_000 {
//           pulse.tick();
//       }
//   });

/// Structure simulant un nœud HNN minimal pour le benchmark.
struct MiniNode {
    position: f64,
    momentum: f64,
    energy: f64,
    turbulence: f64,
}

impl MiniNode {
    fn new(id: u64) -> Self {
        let base = (id as f64) * 0.1;
        Self {
            position: base,
            momentum: 0.0,
            energy: 1.0,
            turbulence: 0.0,
        }
    }

    fn tick(&mut self) {
        // Simulation simplifiée d'un pas Verlet symplectique
        let dt = 0.01;
        let noise = rand::random::<f64>() * 0.001;

        self.momentum += (-self.position.sin() + noise) * dt;
        self.position += self.momentum * dt;
        self.energy = 0.5 * self.momentum.powi(2) + (1.0 - self.position.cos());
        self.turbulence = noise.abs();
    }
}

fn benchmark_hnn(c: &mut Criterion) {
    c.bench_function("hnn 10k ticks", |b| {
        let mut nodes: Vec<MiniNode> = (0..6).map(MiniNode::new).collect();

        b.iter(|| {
            for _ in 0..10_000 {
                for node in &mut nodes {
                    node.tick();
                }
            }
            black_box(&nodes);
        });
    });
}

criterion_group!(benches, benchmark_hnn);
criterion_main!(benches);
