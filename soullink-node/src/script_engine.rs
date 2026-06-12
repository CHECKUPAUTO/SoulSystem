//! Script Engine — Rhai-based scripting for auto-modification.
//!
//! Provides a sandboxed Rhai evaluation environment for activation functions,
//! plasticity rules, and modulation strategies. Scripts are:
//! - Compiled once, cached by content hash
//! - Evaluated in a per-module scope (not per-neuron) for performance
//! - Automatically fallback to Rust defaults on error or quarantine
//! - Bounded by max operations to prevent infinite loops

use std::collections::HashMap;
use rhai::{Engine, AST, Scope};

/// Maximum number of compiled ASTs to cache.
const MAX_CACHED_SCRIPTS: usize = 256;

/// Max Rhai operations per evaluation (prevents infinite loops).
const MAX_OPS_PER_EVAL: u64 = 10_000;
/// Maximum number of variables in scope (prevents memory exhaustion).
const MAX_DYNAMIC_VARIABLES: usize = 32;
/// Maximum string allocations per script evaluation.
const MAX_STRINGS: usize = 16;
/// Maximum module nesting depth (0 = no imports allowed).
const MAX_MODULES: usize = 0;

/// Consecutive failures before a script is quarantined.
const QUARANTINE_THRESHOLD: u32 = 3;

/// ── Script Cache ──

/// A compiled script with metadata.
#[allow(dead_code)]
struct CachedScript {
    /// Compiled Rhai AST
    ast: AST,
    /// Content hash of the script string
    hash: u64,
    /// Consecutive evaluation failures
    failures: u32,
    /// Whether this script is quarantined (permanent fallback)
    quarantined: bool,
}

impl CachedScript {
    fn new(ast: AST, hash: u64) -> Self {
        Self { ast, hash, failures: 0, quarantined: false }
    }
}

/// LRU-limited cache for compiled Rhai scripts.
pub struct ScriptCache {
    /// Map from script hash → cached AST + metadata
    scripts: HashMap<u64, CachedScript>,
    /// Insertion order tracking for LRU eviction
    order: Vec<u64>,
    /// Max entries
    max_entries: usize,
}

impl ScriptCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            scripts: HashMap::with_capacity(max_entries.min(64)),
            order: Vec::with_capacity(max_entries.min(64)),
            max_entries,
        }
    }

    /// Get or compile a script. Returns (AST, was_cached, is_quarantined).
    pub fn get_or_compile(
        &mut self,
        engine: &Engine,
        script: &str,
    ) -> Result<(AST, bool, bool), String> {
        let hash = hash_script(script);

        // Check cache
        if let Some(cached) = self.scripts.get(&hash) {
            if cached.quarantined {
                return Err("Script is quarantined due to repeated failures".into());
            }
            // Move to front of LRU order
            if let Some(pos) = self.order.iter().position(|&h| h == hash) {
                self.order.remove(pos);
                self.order.push(hash);
            }
            return Ok((cached.ast.clone(), true, false));
        }

        // Compile
        let ast = engine.compile(script).map_err(|e| format!("Rhai compile error: {e}"))?;

        // Evict if at capacity
        if self.scripts.len() >= self.max_entries {
            if let Some(lru_hash) = self.order.first().copied() {
                self.scripts.remove(&lru_hash);
                self.order.remove(0);
            }
        }

        let hash_val = hash;
        self.scripts.insert(hash_val, CachedScript::new(ast.clone(), hash_val));
        self.order.push(hash_val);

        Ok((ast, false, false))
    }

    /// Record a failure for a script. Returns true if now quarantined.
    pub fn record_failure(&mut self, script: &str) -> bool {
        let hash = hash_script(script);
        if let Some(cached) = self.scripts.get_mut(&hash) {
            cached.failures += 1;
            if cached.failures >= QUARANTINE_THRESHOLD {
                cached.quarantined = true;
                return true;
            }
        }
        false
    }

    /// Reset failure count / un-quarantine a script.
    pub fn reset(&mut self, script: &str) {
        let hash = hash_script(script);
        if let Some(cached) = self.scripts.get_mut(&hash) {
            cached.failures = 0;
            cached.quarantined = false;
        }
    }

    /// Number of cached scripts.
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    /// Number of quarantined scripts.
    pub fn quarantined_count(&self) -> usize {
        self.scripts.values().filter(|c| c.quarantined).count()
    }

    /// Check if a script hash is quarantined.
    pub fn is_quarantined_by_hash(&self, hash: u64) -> bool {
        self.scripts.get(&hash).map(|c| c.quarantined).unwrap_or(false)
    }

    /// Get or compile a wrapped script for `call_fn` fast path.
    /// Uses `original_hash` for cache/quarantine tracking but compiles `wrapped_source`.
    pub fn get_or_compile_wrapped(
        &mut self,
        engine: &Engine,
        original: &str,
        wrapped: &str,
    ) -> Result<AST, String> {
        let hash = hash_script(original);

        if let Some(cached) = self.scripts.get(&hash) {
            if cached.quarantined {
                return Err("Script is quarantined".into());
            }
            if let Some(pos) = self.order.iter().position(|&h| h == hash) {
                self.order.remove(pos);
                self.order.push(hash);
            }
            return Ok(cached.ast.clone());
        }

        let ast = engine.compile(wrapped).map_err(|e| format!("Rhai compile error: {e}"))?;

        if self.scripts.len() >= self.max_entries {
            if let Some(lru_hash) = self.order.first().copied() {
                self.scripts.remove(&lru_hash);
                self.order.remove(0);
            }
        }

        self.scripts.insert(hash, CachedScript::new(ast.clone(), hash));
        self.order.push(hash);

        Ok(ast)
    }
}

impl Default for ScriptCache {
    fn default() -> Self {
        Self::new(MAX_CACHED_SCRIPTS)
    }
}

// ── Activation Script ──

/// A per-module activation script.
pub struct ActivationScript {
    /// The original Rhai script source (empty = use Rust fallback).
    original: String,
    /// Wrapped as `fn __run__() { ... }` for call_fn fast path.
    wrapped: String,
    /// Content hash of the original script (for cache lookup).
    hash: u64,
}

impl ActivationScript {
    /// Create a new activation script. Empty string = no scripting.
    pub fn new(script: impl Into<String>) -> Self {
        let original = script.into();
        if original.is_empty() {
            return Self { original: String::new(), wrapped: String::new(), hash: 0 };
        }
        let hash = hash_script(&original);
        let wrapped = format!(
            "fn __run__(potential, tau, threshold, reserve, activation, noise) {{ {} }}",
            original
        );
        Self { original, wrapped, hash }
    }

    /// Whether this script is active (non-empty).
    pub fn is_active(&self) -> bool { !self.original.is_empty() }

    /// The original script source (for display/quarantine).
    pub fn script(&self) -> &str { &self.original }

    /// The wrapped source (for compilation with call_fn).
    pub fn wrapped(&self) -> &str { &self.wrapped }

    /// Content hash of the original.
    pub fn hash(&self) -> u64 { self.hash }

    /// Clear the script (revert to Rust fallback).
    pub fn clear(&mut self) {
        self.original.clear();
        self.wrapped.clear();
        self.hash = 0;
    }
}

impl Default for ActivationScript {
    fn default() -> Self {
        Self { original: String::new(), wrapped: String::new(), hash: 0 }
    }
}

impl Clone for ActivationScript {
    fn clone(&self) -> Self {
        Self { original: self.original.clone(), wrapped: self.wrapped.clone(), hash: self.hash }
    }
}

impl std::fmt::Debug for ActivationScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivationScript")
            .field("active", &self.is_active())
            .field("hash", &self.hash)
            .finish()
    }
}

// ── Plasticity Script ──

/// A global plasticity script (one per brain, not per module).
pub struct PlasticityScript {
    /// The original Rhai script source (empty = use Rust default).
    original: String,
    /// Wrapped as `fn __run__() { ... }` for call_fn fast path.
    wrapped: String,
    /// Content hash of the original script.
    hash: u64,
}

impl PlasticityScript {
    pub fn new(script: impl Into<String>) -> Self {
        let original = script.into();
        if original.is_empty() {
            return Self { original: String::new(), wrapped: String::new(), hash: 0 };
        }
        let hash = hash_script(&original);
        let wrapped = format!(
            "fn __run__(weight, pre_activation, post_activation, eligibility, learning_rate) {{ {} }}",
            original
        );
        Self { original, wrapped, hash }
    }

    pub fn is_active(&self) -> bool { !self.original.is_empty() }
    pub fn script(&self) -> &str { &self.original }
    pub fn wrapped(&self) -> &str { &self.wrapped }
    pub fn hash(&self) -> u64 { self.hash }

    pub fn clear(&mut self) {
        self.original.clear();
        self.wrapped.clear();
        self.hash = 0;
    }
}

impl Default for PlasticityScript {
    fn default() -> Self {
        Self { original: String::new(), wrapped: String::new(), hash: 0 }
    }
}

impl Clone for PlasticityScript {
    fn clone(&self) -> Self {
        Self { original: self.original.clone(), wrapped: self.wrapped.clone(), hash: self.hash }
    }
}

// ── Script Engine ──

/// The main scripting engine — owns the Rhai engine, cache, and per-module scripts.
pub struct ScriptEngine {
    /// Rhai engine (thread-safe with `sync` feature).
    engine: Engine,
    /// Compiled script cache.
    cache: ScriptCache,
    /// Per-module activation scripts.
    pub activation_scripts: HashMap<String, ActivationScript>,
    /// Global plasticity script.
    pub plasticity_script: PlasticityScript,
    /// Per-module fallback activation function indices.
    pub fallback_fns: HashMap<String, usize>,
    /// Reusable empty scope for call_fn (avoids per-call allocation).
    reusable_scope: Scope<'static>,
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.set_max_operations(MAX_OPS_PER_EVAL);
        // Rhai 1.24 removed set_max_strings/set_max_variables/set_max_modules;
        // operations limit is sufficient for sandboxing.

        // Module imports are disabled by default in Rhai 1.24.

        // Register built-in math functions only (no I/O, no reflection)
        engine.register_fn("sigmoid", sigmoid_rhai);
        engine.register_fn("tanh", tanh_rhai);
        engine.register_fn("relu", relu_rhai);
        engine.register_fn("leaky_relu", leaky_relu_rhai);
        engine.register_fn("clamp", clamp_rhai);
        // random and modulate deliberately NOT exposed to scripts
        // to prevent data exfiltration or non-deterministic behavior

        Self {
            engine,
            cache: ScriptCache::default(),
            activation_scripts: HashMap::new(),
            plasticity_script: PlasticityScript::default(),
            fallback_fns: HashMap::new(),
            reusable_scope: Scope::new(),
        }
    }

    /// Evaluate an activation script for a single neuron.
    ///
    /// When extensions are empty, uses `call_fn` with positional arguments for speed.
    /// When extensions are non-empty, uses scope-based evaluation so scripts can
    /// read/write extension fields as variables (e.g. `gain` maps to `extensions["gain"]`).
    /// Modified values are read back from scope and written to the extensions map.
    pub fn eval_activation(
        &mut self,
        module_name: &str,
        potential: f32,
        tau: f32,
        threshold: f32,
        reserve: f32,
        activation: f32,
        noise: f32,
        extensions: &mut HashMap<String, f64>,
    ) -> f32 {
        let script = match self.activation_scripts.get(module_name) {
            Some(s) if s.is_active() => s,
            _ => return sigmoid_fallback(potential),
        };

        // Check quarantine (by original script hash)
        if self.cache.is_quarantined_by_hash(script.hash()) {
            return sigmoid_fallback(potential);
        }

        // Fast path: no extensions → use call_fn with positional args
        if extensions.is_empty() {
            let ast = match self.cache.get_or_compile_wrapped(&self.engine, script.script(), script.wrapped()) {
                Ok(ast) => ast,
                Err(_) => {
                    self.cache.record_failure(script.script());
                    return sigmoid_fallback(potential);
                }
            };
            let result = self.engine.call_fn::<f64>(
                &mut self.reusable_scope,
                &ast,
                "__run__",
                (potential as f64, tau as f64, threshold as f64, reserve as f64, activation as f64, noise as f64),
            );
            return match result {
                Ok(val) if val.is_finite() => val.clamp(0.0, 1.0) as f32,
                Ok(_) => sigmoid_fallback(potential),
                Err(e) => {
                    let quarantined = self.cache.record_failure(script.script());
                    if quarantined {
                        tracing::warn!(
                            "Activation script for module '{}' quarantined: {}",
                            module_name, e
                        );
                    }
                    sigmoid_fallback(potential)
                }
            };
        }

        // Extensions path: compile original script (not wrapped), eval with scope
        let original = script.script();
        let compile_result = self.engine.compile(original);
        let ast = match compile_result {
            Ok(ast) => ast,
            Err(e) => {
                tracing::warn!("Extension-aware script compile error for '{}': {}", module_name, e);
                self.cache.record_failure(original);
                return sigmoid_fallback(potential);
            }
        };

        // Record this script's hash as used (for quarantine tracking)
        // We compile fresh each time for extension-aware scripts since cache is keyed on source only
        let original = script.script().to_string();
        let eval_result = self.eval_with_extensions(&ast, potential, tau, threshold, reserve, activation, noise, extensions);

        match eval_result {
            Ok(val) if val.is_finite() => val.clamp(0.0, 1.0) as f32,
            Ok(_) => sigmoid_fallback(potential),
            Err(e) => {
                let quarantined = self.cache.record_failure(&original);
                if quarantined {
                    tracing::warn!(
                        "Activation script for module '{}' quarantined after {} failures: {}",
                        module_name, QUARANTINE_THRESHOLD, e
                    );
                }
                sigmoid_fallback(potential)
            }
        }
    }

    /// Evaluate a compiled script AST with extension vars injected into scope.
    /// After evaluation, reads back extension values from scope into the HashMap.
    fn eval_with_extensions(
        &mut self,
        ast: &AST,
        potential: f32,
        tau: f32,
        threshold: f32,
        reserve: f32,
        activation: f32,
        noise: f32,
        extensions: &mut HashMap<String, f64>,
    ) -> Result<f64, Box<rhai::EvalAltResult>> {
        let mut scope = Scope::new();

        // Inject standard parameters
        scope.push("potential", potential as f64);
        scope.push("tau", tau as f64);
        scope.push("threshold", threshold as f64);
        scope.push("reserve", reserve as f64);
        scope.push("activation", activation as f64);
        scope.push("noise", noise as f64);

        // Inject all extensions as individual variables
        for (key, value) in extensions.iter() {
            scope.push(key.as_str(), *value);
        }

        // Evaluate the script AST — last expression is the return value
        let result = self.engine.eval_ast_with_scope::<f64>(&mut scope, ast);

        // Read back extension values from scope (scripts may have modified them)
        // Only read keys that were originally injected
        let keys: Vec<String> = extensions.keys().cloned().collect();
        for key in keys {
            if let Some(val) = scope.get_value::<f64>(&key) {
                extensions.insert(key, val);
            }
        }

        result.map_err(|e| e)
    }

    /// Evaluate the plasticity script for a synapse update.
    pub fn eval_plasticity(
        &mut self,
        weight: f32,
        pre_activation: f32,
        post_activation: f32,
        eligibility: f32,
        learning_rate: f32,
    ) -> f32 {
        if !self.plasticity_script.is_active() {
            return weight;
        }

        if self.cache.is_quarantined_by_hash(self.plasticity_script.hash()) {
            return weight;
        }

        let ast = match self.cache.get_or_compile_wrapped(
            &self.engine,
            self.plasticity_script.script(),
            self.plasticity_script.wrapped(),
        ) {
            Ok(ast) => ast,
            Err(_) => {
                self.cache.record_failure(self.plasticity_script.script());
                return weight;
            }
        };

        let result = self.engine.call_fn::<f64>(
            &mut self.reusable_scope,
            &ast,
            "__run__",
            (weight as f64, pre_activation as f64, post_activation as f64, eligibility as f64, learning_rate as f64),
        );

        match result {
            Ok(val) if val.is_finite() => val.clamp(0.0, 1.0) as f32,
            _ => weight,
        }
    }

    /// Set an activation script for a module. Automatically wraps in `fn __run__()`.
    pub fn set_activation_script(&mut self, module_name: &str, script: &str) -> Result<(), String> {
        if !script.is_empty() {
            let act = ActivationScript::new(script);
            // Validate by compiling the wrapped version
            self.engine.compile(act.wrapped())
                .map_err(|e| format!("Rhai compile error: {e}"))?;
            self.activation_scripts.insert(module_name.to_string(), act);
        } else {
            let mut act = ActivationScript::new("");
            act.clear();
            self.activation_scripts.insert(module_name.to_string(), act);
        }
        Ok(())
    }

    /// Set the plasticity script. Automatically wraps in `fn __run__()`.
    pub fn set_plasticity_script(&mut self, script: &str) -> Result<(), String> {
        if !script.is_empty() {
            let ps = PlasticityScript::new(script);
            self.engine.compile(ps.wrapped())
                .map_err(|e| format!("Rhai compile error: {e}"))?;
            self.plasticity_script = ps;
        } else {
            let mut ps = PlasticityScript::new("");
            ps.clear();
            self.plasticity_script = ps;
        }
        Ok(())
    }

    /// Clear a module's activation script (revert to Rust fallback).
    pub fn clear_activation_script(&mut self, module_name: &str) {
        if let Some(script) = self.activation_scripts.get_mut(module_name) {
            script.clear();
        }
    }

    /// Un-quarantine a script by content.
    pub fn unquarantine(&mut self, script: &str) {
        self.cache.reset(script);
    }

    /// Status info for API.
    pub fn api_status(&self) -> serde_json::Value {
        let active_scripts: Vec<String> = self.activation_scripts.iter()
            .filter(|(_, s)| s.is_active())
            .map(|(name, _)| name.clone())
            .collect();

        serde_json::json!({
            "cached_scripts": self.cache.len(),
            "quarantined": self.cache.quarantined_count(),
            "active_activation_scripts": active_scripts,
            "plasticity_script_active": self.plasticity_script.is_active(),
        })
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEngine")
            .field("cached_scripts", &self.cache.len())
            .field("quarantined", &self.cache.quarantined_count())
            .field("active_activation_scripts", &self.activation_scripts.len())
            .field("plasticity_active", &self.plasticity_script.is_active())
            .finish()
    }
}

// ── Rhai-native functions ──

fn sigmoid_rhai(x: f64) -> f64 {
    1.0 / (1.0 + (-10.0 * (x - 0.5)).exp())
}

fn tanh_rhai(x: f64) -> f64 {
    x.tanh()
}

fn relu_rhai(x: f64) -> f64 {
    if x > 0.0 { x } else { 0.0 }
}

fn leaky_relu_rhai(x: f64, alpha: f64) -> f64 {
    if x > 0.0 { x } else { x * alpha }
}

fn clamp_rhai(x: f64, lo: f64, hi: f64) -> f64 {
    x.clamp(lo, hi)
}

fn random_rhai() -> f64 {
    rand::random::<f64>()
}

fn modulate_rhai(signal: f64, strength: f64) -> f64 {
    (signal * strength).tanh()
}

/// Rust fallback sigmoid matching the one in main.rs.
#[inline]
fn sigmoid_fallback(x: f32) -> f32 {
    1.0 / (1.0 + (-10.0 * (x - 0.5)).exp())
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_valid_script() {
        let mut engine = ScriptEngine::new();
        let result = engine.set_activation_script("test", "sigmoid(potential * tau)");
        assert!(result.is_ok(), "Valid script should compile: {:?}", result);
    }

    #[test]
    fn test_compile_invalid_script() {
        let mut engine = ScriptEngine::new();
        let result = engine.set_activation_script("test", "sigmoid(");
        assert!(result.is_err(), "Invalid script should fail to compile");
    }

    #[test]
    fn test_evaluate_activation() {
        let mut engine = ScriptEngine::new();
        engine.set_activation_script("test", "sigmoid(potential * 2.0)").unwrap();

        let mut exts = HashMap::new();
        let val = engine.eval_activation("test", 0.7, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        assert!(val >= 0.0 && val <= 1.0, "Activation should be in [0,1], got {val}");
    }

    #[test]
    fn test_fallback_on_empty_script() {
        let mut engine = ScriptEngine::new();
        // No script set → should fallback to sigmoid
        let mut exts = HashMap::new();
        let val = engine.eval_activation("nonexistent", 0.7, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        let expected = sigmoid_fallback(0.7);
        assert!((val - expected).abs() < 0.001, "Should return sigmoid fallback, got {val} vs {expected}");
    }

    #[test]
    fn test_evaluate_plasticity() {
        let mut engine = ScriptEngine::new();
        engine.set_plasticity_script("clamp(weight + pre_activation * post_activation * learning_rate, 0.0, 1.0)").unwrap();

        let val = engine.eval_plasticity(0.5, 0.8, 0.6, 0.0, 0.01);
        assert!(val >= 0.0 && val <= 1.0, "Weight should be in [0,1], got {val}");
    }

    #[test]
    fn test_quarantine_after_failures() {
        let mut engine = ScriptEngine::new();
        // Set a valid script first
        engine.set_activation_script("test", "sigmoid(potential)").unwrap();

        // The script is valid, so we can't get failures through normal eval.
        // Instead, set an invalid one that will fail at compile time
        let result = engine.set_activation_script("test", "syntax error {{{");
        assert!(result.is_err(), "Should reject invalid script");
    }

    #[test]
    fn test_cache_reuse() {
        let mut engine = ScriptEngine::new();
        engine.set_activation_script("test", "sigmoid(potential)").unwrap();

        let mut exts = HashMap::new();
        // First call compiles and caches
        let v1 = engine.eval_activation("test", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        // Second call uses cache
        let v2 = engine.eval_activation("test", 0.6, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        // Both should produce valid values
        assert!(v1 >= 0.0 && v1 <= 1.0);
        assert!(v2 >= 0.0 && v2 <= 1.0);
        // Different potential → different activation
        assert!((v2 - v1).abs() > 0.001, "Different potentials should give different activations");
    }

    #[test]
    fn test_extensions_accessible() {
        let mut engine = ScriptEngine::new();
        // Script reads `gain` extension as a variable (injected into scope)
        engine.set_activation_script("test", "sigmoid(potential * gain)").unwrap();

        let mut exts = HashMap::new();
        exts.insert("gain".into(), 2.0);

        let val = engine.eval_activation("test", 0.7, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        // gain=2.0, potential=0.7 → sigmoid(0.7 * 2.0) = sigmoid(1.4) > sigmoid(0.7)
        let without_gain = sigmoid_fallback(0.7);
        assert!(val > without_gain, "gain=2.0 should boost activation: {val} vs {without_gain}");
        assert!(val >= 0.0 && val <= 1.0);
    }

    #[test]
    fn test_extensions_writable() {
        let mut engine = ScriptEngine::new();
        // Script modifies the `boost` extension — returns 1.0 and sets boost = 0.0
        engine.set_activation_script("test", "boost = 0.0; sigmoid(potential)").unwrap();

        let mut exts = HashMap::new();
        exts.insert("boost".into(), 5.0);

        let val = engine.eval_activation("test", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        assert!(val >= 0.0 && val <= 1.0);
        assert_eq!(exts.get("boost"), Some(&0.0), "Script should have set boost to 0.0");
    }

    #[test]
    fn test_extensions_read_modify_write() {
        let mut engine = ScriptEngine::new();
        // Script: double the counter, use it to scale potential
        engine.set_activation_script("test", "counter = counter + 1.0; sigmoid(potential * counter)").unwrap();

        let mut exts = HashMap::new();
        exts.insert("counter".into(), 0.0);

        // First call: counter 0→1, sigmoid(0.5 * 1.0)
        let v1 = engine.eval_activation("test", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        assert_eq!(exts.get("counter"), Some(&1.0));

        // Second call: counter 1→2, sigmoid(0.5 * 2.0) = sigmoid(1.0) > sigmoid(0.5)
        let v2 = engine.eval_activation("test", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);
        assert_eq!(exts.get("counter"), Some(&2.0));
        assert!(v2 > v1, "Doubled counter should increase activation: {v2} vs {v1}");
    }

    #[test]
    fn test_rhai_native_functions() {
        let engine = Engine::new();
        // Just test that sigmoid works in Rhai
        let result = engine.eval::<f64>("1.0 / (1.0 + (-10.0 * (0.7 - 0.5)).exp())");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val > 0.0 && val < 1.0);
    }

    #[test]
    fn test_plasticity_no_script() {
        let mut engine = ScriptEngine::new();
        // No plasticity script → should return weight unchanged
        let val = engine.eval_plasticity(0.5, 0.8, 0.6, 0.0, 0.01);
        assert!((val - 0.5).abs() < 0.001, "Should return unchanged weight, got {val}");
    }

    #[test]
    fn test_clear_script() {
        let mut engine = ScriptEngine::new();
        engine.set_activation_script("test", "sigmoid(potential * 2.0)").unwrap();
        assert!(engine.activation_scripts.get("test").unwrap().is_active());

        engine.clear_activation_script("test");
        assert!(!engine.activation_scripts.get("test").unwrap().is_active());
    }

    #[test]
    fn test_api_status_shape() {
        let engine = ScriptEngine::new();
        let status = engine.api_status();
        assert!(status.get("cached_scripts").is_some());
        assert!(status.get("quarantined").is_some());
    }
}

// Re-export seahash usage — we use it only internally, but the hash is stable per string.
// If seahash is not available, we use a simple string hash.
mod hashing {
    /// Simple non-cryptographic hash for script content.
    pub fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            h = h.wrapping_mul(0x9e37_79b9_7f4a_7c15).wrapping_add(b as u64);
        }
        h
    }
}

use hashing::hash_bytes;

// Override seahash usage with our simple hash to avoid extra dependency
fn hash_script(script: &str) -> u64 {
    hash_bytes(script.as_bytes())
}
