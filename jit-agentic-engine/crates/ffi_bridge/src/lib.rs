//! Contrat ABI pour les Skills JIT.
//! Ce bridge assure que le Host et le Skill s'entendent sur la disposition mémoire.

#[repr(C)]
pub struct SkillContext {
    pub version: u32,
    pub flags: u64,
}

/// Signature standard du point d'entrée pour tout skill compilé.
/// - input_ptr : Pointeur vers le buffer de données source (Zero-copy)
/// - output_ptr : Pointeur vers le buffer de destination (Zero-copy)
/// - len : Longueur des données
pub type SkillExecuteFn = extern "C" fn(input_ptr: *const u8, output_ptr: *mut u8, len: usize);
