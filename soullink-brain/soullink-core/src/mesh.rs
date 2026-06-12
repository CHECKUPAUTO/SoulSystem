//! SoulLink Neural Mesh - Structure of Arrays (SoA) layout.

pub struct OrganMesh {
    pub q: Vec<f32>, // Positions (cognitive state)
    pub p: Vec<f32>, // Momenta (rate of change)
}

impl OrganMesh {
    pub fn new(size: usize) -> Self {
        Self {
            q: vec![0.0; size],
            p: vec![0.0; size],
        }
    }
}

pub struct SoulLinkMesh {
    pub science: OrganMesh,
    pub mind: OrganMesh,
    pub engineer: OrganMesh,
    pub crypto: OrganMesh,
    pub creative: OrganMesh,
    pub meta: OrganMesh,
}

impl SoulLinkMesh {
    pub fn new(size: usize) -> Self {
        Self {
            science: OrganMesh::new(size),
            mind: OrganMesh::new(size),
            engineer: OrganMesh::new(size),
            crypto: OrganMesh::new(size),
            creative: OrganMesh::new(size),
            meta: OrganMesh::new(size),
        }
    }
}
