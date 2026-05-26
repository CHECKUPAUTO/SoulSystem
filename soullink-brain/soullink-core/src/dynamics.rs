//! Hamiltonian dynamics integration for the Neural Mesh.
use crate::mesh::OrganMesh;

/// Integrates the Hamiltonian system for one time step using Leapfrog method.
/// Ensures conservation of energy (Symplectic integration).
pub fn integrate_hamiltonian(organ: &mut OrganMesh, dt: f32) {
    let n = organ.q.len();

    // Leapfrog:
    // 1. p(t + dt/2) = p(t) - (dH/dq) * (dt/2)
    // 2. q(t + dt) = q(t) + (dH/dp) * dt
    // 3. p(t + dt) = p(t + dt/2) - (dH/dq) * (dt/2)

    for i in 0..n {
        // Mock Hamiltonian: H = p^2 / 2 + q^2 / 2 (Harmonic Oscillator)
        // dH/dq = q, dH/dp = p

        let dq = organ.q[i];
        organ.p[i] -= dq * (dt / 2.0);

        organ.q[i] += organ.p[i] * dt;

        let dq_new = organ.q[i];
        organ.p[i] -= dq_new * (dt / 2.0);
    }
}

pub fn calculate_energy(organ: &OrganMesh) -> f32 {
    organ.q.iter().zip(organ.p.iter()).map(|(&q, &p)| {
        0.5 * (p * p + q * q)
    }).sum()
}
