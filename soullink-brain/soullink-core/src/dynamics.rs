//! Hamiltonian dynamics for the SoulLink Neural Mesh.
//!
//! The mesh is a network of coupled oscillators. Each node `i` has a
//! generalized coordinate `q_i` (cognitive state) and momentum `p_i` (rate of
//! change). Nodes couple to their neighbours in a ring topology, so the
//! Hamiltonian is that of a discrete linear field (a harmonic chain /
//! Klein–Gordon lattice):
//!
//! ```text
//! H(q, p) = Σ_i [ p_i²/2 + q_i²/2 ]  +  (k/2) Σ_i (q_{i+1} − q_i)²
//!            └── kinetic ──┘└ self ┘      └──── neighbour coupling ────┘
//! ```
//!
//! Integration uses **velocity-Verlet**, a symplectic (Störmer–Verlet) scheme:
//! it conserves the Hamiltonian up to a bounded O(dt²) oscillation with *no
//! secular energy drift* over long runs, and it is exactly time-reversible.
//! Unlike a bag of independent oscillators, the coupling term makes energy
//! genuinely flow between nodes — the property that makes this a *mesh*.

use crate::mesh::{OrganMesh, SoulLinkMesh};

/// Neighbour coupling strength `k`. `0.0` recovers independent harmonic
/// oscillators; larger values couple the mesh more tightly.
pub const COUPLING_K: f32 = 0.5;

/// Force on node `i`, i.e. `−∂H/∂q_i`.
///
/// `−q_i − k·(2·q_i − q_{i−1} − q_{i+1})` for a ring of `n ≥ 2` nodes (the
/// coupling term is the discrete Laplacian). For `n == 1` it degenerates to a
/// single harmonic oscillator (`−q_0`).
#[inline]
fn force(q: &[f32], i: usize) -> f32 {
    let n = q.len();
    if n == 1 {
        return -q[0];
    }
    let left = q[(i + n - 1) % n];
    let right = q[(i + 1) % n];
    -q[i] - COUPLING_K * (2.0 * q[i] - left - right)
}

/// Advance one [`OrganMesh`] by a single time step `dt` using velocity-Verlet.
///
/// Symplectic and time-reversible; total energy ([`calculate_energy`]) is
/// conserved up to a bounded O(dt²) oscillation.
pub fn integrate_hamiltonian(organ: &mut OrganMesh, dt: f32) {
    let n = organ.q.len();
    if n == 0 {
        return;
    }
    // Forces at the current positions.
    let f0: Vec<f32> = (0..n).map(|i| force(&organ.q, i)).collect();
    // Half kick: p += (dt/2)·F(q).
    for i in 0..n {
        organ.p[i] += 0.5 * dt * f0[i];
    }
    // Drift: q += dt·p.
    for i in 0..n {
        organ.q[i] += dt * organ.p[i];
    }
    // Forces at the new positions, then the second half kick.
    let f1: Vec<f32> = (0..n).map(|i| force(&organ.q, i)).collect();
    for i in 0..n {
        organ.p[i] += 0.5 * dt * f1[i];
    }
}

/// Total energy `H(q, p)` of an organ: kinetic + self-potential + neighbour
/// coupling. This is the conserved quantity of [`integrate_hamiltonian`].
pub fn calculate_energy(organ: &OrganMesh) -> f32 {
    let n = organ.q.len();
    let kinetic: f32 = organ.p.iter().map(|&p| 0.5 * p * p).sum();
    let self_pot: f32 = organ.q.iter().map(|&q| 0.5 * q * q).sum();
    let coupling: f32 = if n > 1 {
        (0..n)
            .map(|i| {
                let d = organ.q[(i + 1) % n] - organ.q[i];
                0.5 * COUPLING_K * d * d
            })
            .sum()
    } else {
        0.0
    };
    kinetic + self_pot + coupling
}

impl OrganMesh {
    /// Advance this organ by one Verlet step of size `dt`.
    pub fn step(&mut self, dt: f32) {
        integrate_hamiltonian(self, dt);
    }

    /// Total Hamiltonian energy of this organ.
    pub fn energy(&self) -> f32 {
        calculate_energy(self)
    }
}

impl SoulLinkMesh {
    /// Advance every organ of the mesh by one Verlet step and return the mesh's
    /// total energy afterwards.
    pub fn step(&mut self, dt: f32) -> f32 {
        for organ in self.organs_mut() {
            organ.step(dt);
        }
        self.total_energy()
    }

    /// Total energy summed over all six organs.
    pub fn total_energy(&self) -> f32 {
        self.organs().iter().map(|o| o.energy()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seed an organ with a deterministic, non-trivial state.
    fn excited(n: usize) -> OrganMesh {
        let mut m = OrganMesh::new(n);
        for i in 0..n {
            m.q[i] = ((i as f32) * 0.7).sin();
            m.p[i] = ((i as f32) * 1.3).cos() * 0.3;
        }
        m
    }

    #[test]
    fn energy_conserved_over_long_integration() {
        let mut m = excited(16);
        let e0 = calculate_energy(&m);
        assert!(e0 > 0.0);
        for _ in 0..20_000 {
            integrate_hamiltonian(&mut m, 0.01);
        }
        let e1 = calculate_energy(&m);
        // Symplectic hallmark: energy stays in a bounded band, no secular drift.
        let rel_drift = (e1 - e0).abs() / e0;
        assert!(
            rel_drift < 1e-2,
            "energy drifted too much: {e0} -> {e1} ({rel_drift})"
        );
    }

    #[test]
    fn coupling_transfers_energy_between_nodes() {
        // Excite ONLY node 0; with real coupling the disturbance must spread to
        // neighbours (independent oscillators never would).
        let mut m = OrganMesh::new(8);
        m.q[0] = 1.0;
        assert_eq!(m.p[3], 0.0);
        for _ in 0..2_000 {
            integrate_hamiltonian(&mut m, 0.01);
        }
        // Distant nodes have acquired motion → energy propagated through the mesh.
        let far_moved = m.q.iter().skip(1).any(|&q| q.abs() > 1e-3)
            && m.p.iter().skip(1).any(|&p| p.abs() > 1e-3);
        assert!(far_moved, "coupling did not transfer energy: q={:?}", m.q);
    }

    #[test]
    fn integration_is_time_reversible() {
        let mut m = excited(12);
        let (q0, p0) = (m.q.clone(), m.p.clone());
        for _ in 0..500 {
            integrate_hamiltonian(&mut m, 0.01);
        }
        // Reverse momenta and integrate the same number of steps back.
        for p in m.p.iter_mut() {
            *p = -*p;
        }
        for _ in 0..500 {
            integrate_hamiltonian(&mut m, 0.01);
        }
        for (a, b) in m.q.iter().zip(q0.iter()) {
            assert!((a - b).abs() < 1e-3, "not reversible: {a} vs {b}");
        }
        // Momenta return negated.
        for (a, b) in m.p.iter().zip(p0.iter()) {
            assert!((a + b).abs() < 1e-3);
        }
    }

    #[test]
    fn single_node_reduces_to_harmonic_oscillator() {
        let mut m = OrganMesh::new(1);
        m.q[0] = 1.0;
        let e0 = calculate_energy(&m);
        for _ in 0..1_000 {
            integrate_hamiltonian(&mut m, 0.01);
        }
        assert!((calculate_energy(&m) - e0).abs() < 1e-3);
    }

    #[test]
    fn full_mesh_step_conserves_energy() {
        let mut mesh = SoulLinkMesh::new(10);
        // Excite each organ differently.
        for (k, o) in mesh.organs_mut().into_iter().enumerate() {
            o.q[0] = (k as f32 + 1.0) * 0.2;
            o.p[1] = (k as f32) * 0.1;
        }
        let e0 = mesh.total_energy();
        assert!(e0 > 0.0);
        let mut e_last = e0;
        for _ in 0..5_000 {
            e_last = mesh.step(0.01);
        }
        assert!(
            (e_last - e0).abs() / e0 < 1e-2,
            "mesh energy drifted: {e0} -> {e_last}"
        );
    }
}
