//! Hot-reload system for evolved genome.
//!
//! Takes the evolved genome source, compiles it as a `cdylib`,
//! loads the shared library at runtime via `libloading`,
//! and extracts evolved parameters to apply to the brain.

use std::path::{Path, PathBuf};

/// Directory where the genome crate is created and built.
const GENOME_CRATE_DIR: &str = "/tmp/soullink-genome-crate";
/// Name of the shared library output (libgenome.so on Linux).
const LIB_NAME: &str = "libgenome";

/// Exported genome parameters loaded from the compiled library.
#[derive(Debug, Clone, Default)]
pub struct LoadedGenome {
    /// Whether a genome library was successfully loaded
    pub loaded: bool,
    /// Tau membrane time constant
    pub tau: f64,
    /// Homeostatic boost rate
    pub homeostatic_rate: f64,
    /// Synapse pruning threshold
    pub prune_threshold: f64,
    /// STDP learning rate
    pub stdp_learning_rate: f64,
    /// Cortex d_model
    pub cortex_d_model: usize,
    /// Cortex state_dim
    pub cortex_state_dim: usize,
    /// Cortex n_layers
    pub cortex_n_layers: usize,
    /// Cortex modulation strength
    pub cortex_modulation_strength: f64,
    /// Module definitions (name, neuron_count)
    pub modules: Vec<(String, usize)>,
    /// Energy capacity per neuron
    pub energy_capacity: f64,
    /// Base metabolic rate per neuron per tick
    pub base_metabolic_rate: f64,
    /// Energy cost per firing event
    pub firing_energy_cost: f64,
    /// Energy cost per active synapse per tick
    pub synapse_energy_cost: f64,
    /// Energy recharge rate during rest
    pub energy_recharge_rate: f64,
    /// Whether metabolism is enabled
    pub metabolism_enabled: bool,
    /// Whether global energy pool is active
    pub energy_pool_enabled: bool,
    /// Global pool recharge rate per tick
    pub energy_pool_recharge_rate: f64,
    /// Fraction of neuron surplus collected into pool
    pub pool_contribution: f64,
    /// Energy cost per neuron created
    pub neuron_creation_cost: f64,
    /// Energy cost per synapse created
    pub synapse_creation_cost: f64,
    /// Minimum energy to allow growth
    pub growth_energy_threshold: f64,
    /// Rhai activation scripts (module_name, script_source)
    pub scripts: Vec<(String, String)>,
    /// Global Rhai plasticity script (None = use Rust default)
    pub plasticity_script: Option<String>,
}

impl LoadedGenome {
    /// Convert loaded f64 params to f32 for brain application.
    pub fn as_brain_params(&self) -> crate::BrainParams {
        crate::BrainParams {
            tau: self.tau as f32,
            homeostatic_rate: self.homeostatic_rate as f32,
            prune_threshold: self.prune_threshold as f32,
            stdp_learning_rate: self.stdp_learning_rate as f32,
            energy_capacity: self.energy_capacity as f32,
            base_metabolic_rate: self.base_metabolic_rate as f32,
            firing_energy_cost: self.firing_energy_cost as f32,
            synapse_energy_cost: self.synapse_energy_cost as f32,
            energy_recharge_rate: self.energy_recharge_rate as f32,
            metabolism_enabled: self.metabolism_enabled,
            module_budget_enabled: true,
            budget_share: 0.3,
            energy_pool_enabled: self.energy_pool_enabled,
            energy_pool_recharge_rate: self.energy_pool_recharge_rate as f32,
            pool_contribution: self.pool_contribution as f32,
            neuron_creation_cost: self.neuron_creation_cost as f32,
            synapse_creation_cost: self.synapse_creation_cost as f32,
            growth_energy_threshold: self.growth_energy_threshold as f32,
            arousal_decay: 0.995,
            arousal_circadian_sensitivity: 0.5,
            arousal_energy_sensitivity: 0.4,
            set_point_drift_rate: 0.001,
            metabolic_pressure_rate: 0.01,
            metabolic_pressure_decay: 0.95,
            arousal_threshold_mod: 0.3,
        }
    }
}

/// Write a complete cdylib crate to disk from the genome source.
///
/// Returns the path to the Cargo.toml of the generated crate.
pub fn write_genome_crate(genome_source: &str) -> std::io::Result<PathBuf> {
    let dir = Path::new(GENOME_CRATE_DIR);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
    std::fs::create_dir_all(dir)?;

    // Write Cargo.toml for the cdylib
    let cargo_toml = r#"[package]
name = "genome"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
"#;
    std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;

    // Write src/lib.rs with the genome source and C FFI exports
    let lib_rs = format!(
        r#"//! Auto-generated evolved genome — compiled as cdylib for hot-reload.
//! This file is machine-generated. Do not edit manually.

{genome_source}

// ── C FFI exports ──

#[no_mangle]
pub extern "C" fn genome_tau() -> f64 {{ evolved_params().tau }}

#[no_mangle]
pub extern "C" fn genome_homeostatic_rate() -> f64 {{ evolved_params().homeostatic_rate }}

#[no_mangle]
pub extern "C" fn genome_prune_threshold() -> f64 {{ evolved_params().prune_threshold }}

#[no_mangle]
pub extern "C" fn genome_stdp_learning_rate() -> f64 {{ evolved_params().stdp_learning_rate }}

#[no_mangle]
pub extern "C" fn genome_cortex_d_model() -> i32 {{ evolved_params().cortex.d_model as i32 }}

#[no_mangle]
pub extern "C" fn genome_cortex_state_dim() -> i32 {{ evolved_params().cortex.state_dim as i32 }}

#[no_mangle]
pub extern "C" fn genome_cortex_n_layers() -> i32 {{ evolved_params().cortex.n_layers as i32 }}

#[no_mangle]
pub extern "C" fn genome_cortex_modulation_strength() -> f64 {{ evolved_params().cortex.modulation_strength }}

#[no_mangle]
pub extern "C" fn genome_energy_capacity() -> f64 {{ evolved_params().energy_capacity }}

#[no_mangle]
pub extern "C" fn genome_base_metabolic_rate() -> f64 {{ evolved_params().base_metabolic_rate }}

#[no_mangle]
pub extern "C" fn genome_firing_energy_cost() -> f64 {{ evolved_params().firing_energy_cost }}

#[no_mangle]
pub extern "C" fn genome_synapse_energy_cost() -> f64 {{ evolved_params().synapse_energy_cost }}

#[no_mangle]
pub extern "C" fn genome_energy_recharge_rate() -> f64 {{ evolved_params().energy_recharge_rate }}

#[no_mangle]
pub extern "C" fn genome_metabolism_enabled() -> i32 {{ evolved_params().metabolism_enabled as i32 }}

#[no_mangle]
pub extern "C" fn genome_module_count() -> i32 {{ evolved_module_config().len() as i32 }}

#[no_mangle]
pub extern "C" fn genome_module_neuron_count(idx: i32) -> i32 {{
    let modules = evolved_module_config();
    if idx >= 0 && (idx as usize) < modules.len() {{
        modules[idx as usize].neuron_count as i32
    }} else {{
        0
    }}
}}

    // ── Script FFI exports ──

    #[no_mangle]
    pub extern "C" fn genome_script_count() -> i32 {{ evolved_script_count() }}

    #[no_mangle]
    pub extern "C" fn genome_script_name(idx: i32) -> *const std::os::raw::c_char {{
        std::ffi::CString::new(evolved_script_name(idx))
            .unwrap()
            .into_raw()
    }}

    #[no_mangle]
    pub extern "C" fn genome_script_source(idx: i32) -> *const std::os::raw::c_char {{
        std::ffi::CString::new(evolved_script_source(idx))
            .unwrap()
            .into_raw()
    }}

    #[no_mangle]
    pub extern "C" fn genome_plasticity_script_present() -> i32 {{ evolved_plasticity_script_present() }}

    #[no_mangle]
    pub extern "C" fn genome_plasticity_script_source() -> *const std::os::raw::c_char {{
        std::ffi::CString::new(evolved_plasticity_script_source())
            .unwrap()
            .into_raw()
    }}

    #[no_mangle]
    pub extern "C" fn genome_free_string(s: *mut std::os::raw::c_char) {{
        if !s.is_null() {{
            unsafe {{ let _ = std::ffi::CString::from_raw(s); }}
        }}
    }}"#,
        genome_source = genome_source
    );

    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::write(src_dir.join("lib.rs"), lib_rs)?;

    Ok(dir.join("Cargo.toml"))
}

/// Compile the genome crate with `cargo build`.
///
/// Returns the path to the compiled shared library on success.
pub fn compile_genome_crate() -> Result<PathBuf, String> {
    let status = std::process::Command::new("cargo")
        .args(&["build", "--release"])
        .current_dir(GENOME_CRATE_DIR)
        .output()
        .map_err(|e| format!("Failed to run cargo: {}", e))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("Compilation failed:\n{}", stderr));
    }

    // Find the .so file
    let so_path = Path::new(GENOME_CRATE_DIR)
        .join("target")
        .join("release")
        .join(format!("{}.so", LIB_NAME));

    if so_path.exists() {
        Ok(so_path)
    } else {
        // Try .dylib (macOS) or .dll (Windows)
        let dylib_path = Path::new(GENOME_CRATE_DIR)
            .join("target")
            .join("release")
            .join(format!("{}.dylib", LIB_NAME));
        if dylib_path.exists() {
            Ok(dylib_path)
        } else {
            let dll_path = Path::new(GENOME_CRATE_DIR)
                .join("target")
                .join("release")
                .join(format!("{}.dll", LIB_NAME));
            if dll_path.exists() {
                Ok(dll_path)
            } else {
                Err(format!("Shared library not found after compilation (looked for .so/.dylib/.dll)"))
            }
        }
    }
}

/// Load a compiled genome shared library and extract its parameters.
///
/// # Safety
///
/// Loads a shared library and calls its exported C functions.
/// The library must have been compiled from `write_genome_crate` output.
pub unsafe fn load_genome_library(so_path: &Path) -> Result<LoadedGenome, String> {
    let lib = libloading::Library::new(so_path)
        .map_err(|e| format!("Failed to load library: {}", e))?;

    // Helper macro to call a function by symbol name
    macro_rules! call_f64 {
        ($lib:expr, $name:expr) => {{
            let func: libloading::Symbol<extern "C" fn() -> f64> = $lib
                .get($name.as_bytes())
                .map_err(|e| format!("Symbol '{}' not found: {}", $name, e))?;
            func()
        }};
    }

    macro_rules! call_i32 {
        ($lib:expr, $name:expr) => {{
            let func: libloading::Symbol<extern "C" fn() -> i32> = $lib
                .get($name.as_bytes())
                .map_err(|e| format!("Symbol '{}' not found: {}", $name, e))?;
            func()
        }};
    }

    let tau = call_f64!(lib, "genome_tau");
    let homeostatic_rate = call_f64!(lib, "genome_homeostatic_rate");
    let prune_threshold = call_f64!(lib, "genome_prune_threshold");
    let stdp_lr = call_f64!(lib, "genome_stdp_learning_rate");
    let cortex_d_model = call_i32!(lib, "genome_cortex_d_model") as usize;
    let cortex_state_dim = call_i32!(lib, "genome_cortex_state_dim") as usize;
    let cortex_n_layers = call_i32!(lib, "genome_cortex_n_layers") as usize;
    let cortex_mod_strength = call_f64!(lib, "genome_cortex_modulation_strength");
    let module_count = call_i32!(lib, "genome_module_count");

    let energy_capacity = call_f64!(lib, "genome_energy_capacity");
    let base_metabolic_rate = call_f64!(lib, "genome_base_metabolic_rate");
    let firing_energy_cost = call_f64!(lib, "genome_firing_energy_cost");
    let synapse_energy_cost = call_f64!(lib, "genome_synapse_energy_cost");
    let energy_recharge_rate = call_f64!(lib, "genome_energy_recharge_rate");
    let metabolism_enabled = call_i32!(lib, "genome_metabolism_enabled") != 0;

    let mut modules = Vec::new();
    for i in 0..module_count {
        let count_func: libloading::Symbol<extern "C" fn(i32) -> i32> = lib
            .get(b"genome_module_neuron_count")
            .map_err(|e| format!("Symbol 'genome_module_neuron_count' not found: {}", e))?;
        let count = count_func(i);
        modules.push((format!("evolved_{}", i), count as usize));
    }

    // Get free-string helper to avoid leaking CStrings from the .so
    let free_func: libloading::Symbol<extern "C" fn(*mut std::os::raw::c_char)> = lib
        .get(b"genome_free_string")
        .map_err(|e| format!("Symbol 'genome_free_string' not found: {}", e))?;

    // Extract scripts
    let script_count = call_i32!(lib, "genome_script_count");
    let mut scripts = Vec::with_capacity(script_count as usize);
    for i in 0..script_count {
        // Get name
        let name_func: libloading::Symbol<extern "C" fn(i32) -> *const std::os::raw::c_char> = lib
            .get(b"genome_script_name")
            .map_err(|e| format!("Symbol 'genome_script_name' not found: {}", e))?;
        let name_ptr = name_func(i);
        let name = if !name_ptr.is_null() {
            let s = unsafe { std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().into_owned() };
            free_func(name_ptr as *mut _);
            s
        } else {
            format!("script_{}", i)
        };

        // Get source
        let src_func: libloading::Symbol<extern "C" fn(i32) -> *const std::os::raw::c_char> = lib
            .get(b"genome_script_source")
            .map_err(|e| format!("Symbol 'genome_script_source' not found: {}", e))?;
        let src_ptr = src_func(i);
        let source = if !src_ptr.is_null() {
            let s = unsafe { std::ffi::CStr::from_ptr(src_ptr).to_string_lossy().into_owned() };
            free_func(src_ptr as *mut _);
            s
        } else {
            String::new()
        };

        if !source.is_empty() {
            scripts.push((name, source));
        }
    }

    // Extract plasticity script
    let plasticity_present = call_i32!(lib, "genome_plasticity_script_present") != 0;
    let plasticity_script = if plasticity_present {
        let ps_func: libloading::Symbol<extern "C" fn() -> *const std::os::raw::c_char> = lib
            .get(b"genome_plasticity_script_source")
            .map_err(|e| format!("Symbol 'genome_plasticity_script_source' not found: {}", e))?;
        let ps_ptr = ps_func();
        if !ps_ptr.is_null() {
            let s = unsafe { std::ffi::CStr::from_ptr(ps_ptr).to_string_lossy().into_owned() };
            free_func(ps_ptr as *mut _);
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        }
    } else {
        None
    };

    // Prevent dlclose so that any borrowed string data (or vtables) stays mapped.

    // Prevent dlclose so string data stays mapped.
    Ok(LoadedGenome {
        loaded: true,
        tau,
        homeostatic_rate,
        prune_threshold,
        stdp_learning_rate: stdp_lr,
        cortex_d_model,
        cortex_state_dim,
        cortex_n_layers,
        cortex_modulation_strength: cortex_mod_strength,
        modules,
        scripts,
        plasticity_script,
        energy_capacity,
        base_metabolic_rate,
        firing_energy_cost,
        synapse_energy_cost,
        energy_recharge_rate,
        metabolism_enabled,
        energy_pool_enabled: true,
        energy_pool_recharge_rate: 0.01,
        pool_contribution: 0.1,
        neuron_creation_cost: 0.05,
        synapse_creation_cost: 0.01,
        growth_energy_threshold: 0.3,
    })
}


/// ── JSON-based parameter hotload (fast path, no compilation) ──

/// Default genome JSON path.
const JSON_GENOME_PATH: &str = "/tmp/soullink-genome.json";

/// Load genome parameters from a JSON file (fast hotload, no cargo).
///
/// Reads `soullink-genome.json` from `/tmp/` (or `SOULLINK_GENOME_JSON` env var)
/// and deserializes it into a `LoadedGenome`. This is the *primary* hotload
/// path — it completes in microseconds instead of minutes.
///
/// # JSON format
///
/// ```json
/// {
///   "tau": 0.98,
///   "homeostatic_rate": 0.005,
///   "prune_threshold": 0.05,
///   "stdp_learning_rate": 0.005,
///   "cortex_d_model": 16,
///   "cortex_state_dim": 4,
///   "cortex_n_layers": 2,
///   "cortex_modulation_strength": 0.3,
///   "energy_capacity": 1.0,
///   "base_metabolic_rate": 0.0001,
///   "firing_energy_cost": 0.02,
///   "synapse_energy_cost": 0.00001,
///   "energy_recharge_rate": 0.001,
///   "metabolism_enabled": true,
///   "energy_pool_enabled": true,
///   "energy_pool_recharge_rate": 0.01,
///   "pool_contribution": 0.1,
///   "neuron_creation_cost": 0.05,
///   "synapse_creation_cost": 0.01,
///   "growth_energy_threshold": 0.3,
///   "modules": [["perception", 3000], ["memory", 3000], ...],
///   "scripts": [["module_name", "sigmoid(potential * tau)"]],
///   "plasticity_script": null
/// }
/// ```
pub fn load_genome_json() -> Result<LoadedGenome, String> {
    let path = std::env::var("SOULLINK_GENOME_JSON")
        .unwrap_or_else(|_| JSON_GENOME_PATH.to_string());
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err(format!("No genome JSON at {}", path.display()));
    }

    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    #[derive(serde::Deserialize)]
    struct JsonGenome {
        #[serde(default)]
        tau: f64,
        #[serde(default = "default_homeostatic")]
        homeostatic_rate: f64,
        #[serde(default = "default_prune")]
        prune_threshold: f64,
        #[serde(default = "default_stdp")]
        stdp_learning_rate: f64,
        #[serde(default = "default_d_model")]
        cortex_d_model: usize,
        #[serde(default = "default_state_dim")]
        cortex_state_dim: usize,
        #[serde(default = "default_layers")]
        cortex_n_layers: usize,
        #[serde(default = "default_mod_strength")]
        cortex_modulation_strength: f64,
        #[serde(default = "default_energy_cap")]
        energy_capacity: f64,
        #[serde(default = "default_metabolic")]
        base_metabolic_rate: f64,
        #[serde(default = "default_firing_cost")]
        firing_energy_cost: f64,
        #[serde(default = "default_synapse_cost")]
        synapse_energy_cost: f64,
        #[serde(default = "default_recharge")]
        energy_recharge_rate: f64,
        #[serde(default = "default_metabolism_on")]
        metabolism_enabled: bool,
        #[serde(default = "default_pool_on")]
        energy_pool_enabled: bool,
        #[serde(default = "default_pool_rate")]
        energy_pool_recharge_rate: f64,
        #[serde(default = "default_pool_contrib")]
        pool_contribution: f64,
        #[serde(default = "default_neuron_cost")]
        neuron_creation_cost: f64,
        #[serde(default = "default_syn_creation_cost")]
        synapse_creation_cost: f64,
        #[serde(default = "default_growth_threshold")]
        growth_energy_threshold: f64,
        #[serde(default)]
        modules: Vec<[String; 2]>,
        #[serde(default)]
        scripts: Vec<[String; 2]>,
        #[serde(default)]
        plasticity_script: Option<String>,
    }

    fn default_homeostatic() -> f64 { 0.005 }
    fn default_prune() -> f64 { 0.05 }
    fn default_stdp() -> f64 { 0.005 }
    fn default_d_model() -> usize { 16 }
    fn default_state_dim() -> usize { 4 }
    fn default_layers() -> usize { 2 }
    fn default_mod_strength() -> f64 { 0.3 }
    fn default_energy_cap() -> f64 { 1.0 }
    fn default_metabolic() -> f64 { 0.0001 }
    fn default_firing_cost() -> f64 { 0.02 }
    fn default_synapse_cost() -> f64 { 0.00001 }
    fn default_recharge() -> f64 { 0.001 }
    fn default_metabolism_on() -> bool { true }
    fn default_pool_on() -> bool { true }
    fn default_pool_rate() -> f64 { 0.01 }
    fn default_pool_contrib() -> f64 { 0.1 }
    fn default_neuron_cost() -> f64 { 0.05 }
    fn default_syn_creation_cost() -> f64 { 0.01 }
    fn default_growth_threshold() -> f64 { 0.3 }

    let json: JsonGenome = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse genome JSON: {}", e))?;

    let modules: Vec<(String, usize)> = json.modules.iter()
        .map(|m| (m[0].clone(), m[1].parse().unwrap_or(3000)))
        .collect();

    let scripts: Vec<(String, String)> = json.scripts.iter()
        .map(|s| (s[0].clone(), s[1].clone()))
        .collect();

    Ok(LoadedGenome {
        loaded: true,
        tau: json.tau,
        homeostatic_rate: json.homeostatic_rate,
        prune_threshold: json.prune_threshold,
        stdp_learning_rate: json.stdp_learning_rate,
        cortex_d_model: json.cortex_d_model,
        cortex_state_dim: json.cortex_state_dim,
        cortex_n_layers: json.cortex_n_layers,
        cortex_modulation_strength: json.cortex_modulation_strength,
        modules,
        scripts,
        plasticity_script: json.plasticity_script,
        energy_capacity: json.energy_capacity,
        base_metabolic_rate: json.base_metabolic_rate,
        firing_energy_cost: json.firing_energy_cost,
        synapse_energy_cost: json.synapse_energy_cost,
        energy_recharge_rate: json.energy_recharge_rate,
        metabolism_enabled: json.metabolism_enabled,
        energy_pool_enabled: json.energy_pool_enabled,
        energy_pool_recharge_rate: json.energy_pool_recharge_rate,
        pool_contribution: json.pool_contribution,
        neuron_creation_cost: json.neuron_creation_cost,
        synapse_creation_cost: json.synapse_creation_cost,
        growth_energy_threshold: json.growth_energy_threshold,
    })
}

/// Write genome parameters to JSON file for the fast hotload path.
pub fn write_genome_json(genome: &LoadedGenome) -> Result<(), String> {
    let json = serde_json::json!({
        "tau": genome.tau,
        "homeostatic_rate": genome.homeostatic_rate,
        "prune_threshold": genome.prune_threshold,
        "stdp_learning_rate": genome.stdp_learning_rate,
        "cortex_d_model": genome.cortex_d_model,
        "cortex_state_dim": genome.cortex_state_dim,
        "cortex_n_layers": genome.cortex_n_layers,
        "cortex_modulation_strength": genome.cortex_modulation_strength,
        "energy_capacity": genome.energy_capacity,
        "base_metabolic_rate": genome.base_metabolic_rate,
        "firing_energy_cost": genome.firing_energy_cost,
        "synapse_energy_cost": genome.synapse_energy_cost,
        "energy_recharge_rate": genome.energy_recharge_rate,
        "metabolism_enabled": genome.metabolism_enabled,
        "energy_pool_enabled": genome.energy_pool_enabled,
        "energy_pool_recharge_rate": genome.energy_pool_recharge_rate,
        "pool_contribution": genome.pool_contribution,
        "neuron_creation_cost": genome.neuron_creation_cost,
        "synapse_creation_cost": genome.synapse_creation_cost,
        "growth_energy_threshold": genome.growth_energy_threshold,
        "modules": genome.modules.iter().map(|(n, c)| [n.clone(), c.to_string()]).collect::<Vec<_>>(),
        "scripts": genome.scripts.iter().map(|(n, s)| [n.clone(), s.clone()]).collect::<Vec<_>>(),
        "plasticity_script": genome.plasticity_script,
    });

    let path = std::env::var("SOULLINK_GENOME_JSON")
        .unwrap_or_else(|_| JSON_GENOME_PATH.to_string());

    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap())
        .map_err(|e| format!("Failed to write genome JSON: {}", e))?;

    Ok(())
}

/// Full pipeline: generate source → write crate → compile → load.
///
/// Returns the loaded genome parameters on success.
/// Convertit un génome en source Rust compilable (Correction 2: Gödel).
pub fn genome_to_rust_source(genome: &crate::evolution::Genome) -> String {
    genome.to_rust_source()
}

pub fn compile_and_load_genome(genome_source: &str) -> Result<LoadedGenome, String> {
    // Write the crate
    write_genome_crate(genome_source)
        .map_err(|e| format!("Failed to write genome crate: {}", e))?;

    // Compile
    let so_path = compile_genome_crate()?;

    // Load (unsafe)
    unsafe { load_genome_library(&so_path) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_genome_crate_creates_files() {
        let source = "pub fn dummy() -> i32 { 42 }";
        let result = write_genome_crate(source);
        assert!(result.is_ok(), "Should write crate: {:?}", result);
        let path = result.unwrap();
        assert!(path.exists(), "Cargo.toml should exist");
        assert!(path.parent().unwrap().join("src/lib.rs").exists(), "lib.rs should exist");
        // Clean up
        let _ = std::fs::remove_dir_all(GENOME_CRATE_DIR);
    }
}
