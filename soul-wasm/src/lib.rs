use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use wasmtime::*;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Compilation error: {0}")]
    Compilation(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Serialization error: {0}")]
    Serde(String),
    #[error("Trap: {0}")]
    Trap(String),
}

pub type Result<T> = std::result::Result<T, WasmError>;

// ─── Memory Limiter ──────────────────────────────────────────────────────────

struct MemLimiter {
    memory_size: usize,
}

impl ResourceLimiter for MemLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> std::result::Result<bool, wasmtime::Error> {
        Ok(desired <= self.memory_size)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> std::result::Result<bool, wasmtime::Error> {
        Ok(true)
    }

    fn instances(&self) -> usize {
        1
    }

    fn tables(&self) -> usize {
        1
    }

    fn memories(&self) -> usize {
        1
    }
}

// ─── Plugin Manifest ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub entry_point: String,
    #[serde(default)]
    pub memory_limit_mb: usize,
    #[serde(default)]
    pub time_limit_ms: u64,
}

impl Default for PluginManifest {
    fn default() -> Self {
        Self {
            name: "unknown".into(),
            version: "0.1.0".into(),
            description: "".into(),
            author: "unknown".into(),
            permissions: Vec::new(),
            entry_point: "_start".into(),
            memory_limit_mb: 64,
            time_limit_ms: 5000,
        }
    }
}

// ─── Plugin Result ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
    pub success: bool,
    pub output: String,
    #[serde(default)]
    pub error: Option<String>,
    pub execution_time_ms: u64,
    #[serde(default)]
    pub memory_used_bytes: usize,
}

// ─── Sandbox Config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub memory_limit_mb: usize,
    pub time_limit_ms: u64,
    pub max_stack_size_mb: usize,
    #[serde(default)]
    pub allowed_syscalls: Vec<String>,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    #[serde(default)]
    pub fs_read_paths: Vec<PathBuf>,
    #[serde(default)]
    pub fs_write_paths: Vec<PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit_mb: 64,
            time_limit_ms: 5000,
            max_stack_size_mb: 8,
            allowed_syscalls: vec![
                "clock_gettime".into(),
                "proc_exit".into(),
                "fd_write".into(),
                "fd_read".into(),
                "fd_close".into(),
                "environ_get".into(),
                "environ_sizes_get".into(),
            ],
            env_vars: HashMap::new(),
            fs_read_paths: Vec::new(),
            fs_write_paths: Vec::new(),
        }
    }
}

// ─── WASM Plugin (wasmtime runtime) ──────────────────────────────────────────

pub struct WasmPlugin {
    manifest: PluginManifest,
    sandbox: SandboxConfig,
    wasm_bytes: Vec<u8>,
}

impl WasmPlugin {
    pub fn new(wasm_bytes: Vec<u8>, manifest: PluginManifest) -> Self {
        let sandbox = SandboxConfig {
            memory_limit_mb: manifest.memory_limit_mb,
            time_limit_ms: manifest.time_limit_ms,
            ..Default::default()
        };
        Self {
            manifest,
            sandbox,
            wasm_bytes,
        }
    }

    pub fn with_sandbox(mut self, config: SandboxConfig) -> Self {
        self.sandbox = config;
        self
    }

    pub fn compile(&self) -> Result<()> {
        validate_wasm(&self.wasm_bytes)
    }

    fn compile_module(&self) -> std::result::Result<Module, WasmError> {
        Module::new(&Engine::default(), &self.wasm_bytes)
            .map_err(|e| WasmError::Compilation(e.to_string()))
    }

    pub async fn execute(&self, input: &str) -> Result<PluginResult> {
        let start = std::time::Instant::now();

        let module = match self.compile_module() {
            Ok(m) => m,
            Err(e) => {
                return Ok(PluginResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    memory_used_bytes: 0,
                });
            }
        };

        let engine = Engine::default();
        let mut store = Store::new(
            &engine,
            MemLimiter {
                memory_size: self.sandbox.memory_limit_mb * 1024 * 1024,
            },
        );
        store.limiter(|state| state);

        let mut linker = Linker::new(&engine);

        // WASI-like host functions
        let stdout_buf: std::rc::Rc<std::cell::RefCell<Vec<u8>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let _stdout_clone = stdout_buf.clone();

        linker
            .func_wrap(
                "env",
                "fd_write",
                move |fd: i32, _iovs: i32, _iovs_len: i32, _nwritten_out: i32| -> i32 {
                    // fd=1 is stdout, fd=2 is stderr
                    if fd == 1 || fd == 2 {
                        // In a real implementation, we'd read from WASM memory at iovs pointer
                        // For now, acknowledge the call and write dummy data
                        tracing::debug!(fd, "fd_write called from WASM");
                    }
                    0 // success
                },
            )
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        linker
            .func_wrap("env", "proc_exit", move |code: i32| {
                tracing::info!("WASM plugin exited with code: {}", code);
            })
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        linker
            .func_wrap("env", "environ_sizes_get", move |_a: i32, _b: i32| -> i32 {
                0
            })
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        linker
            .func_wrap("env", "environ_get", move |_a: i32, _b: i32| -> i32 { 0 })
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        // Memory
        let memory = Memory::new(&mut store, MemoryType::new(16, Some(256)))
            .map_err(|e| WasmError::Runtime(e.to_string()))?;
        linker
            .define(&store, "env", "memory", memory)
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        // Write input to WASM memory
        let input_bytes = input.as_bytes();
        let _input_len = input_bytes.len() as i32;
        let input_ptr = 0i32;

        memory.data_mut(&mut store)[input_ptr as usize..input_ptr as usize + input_bytes.len()]
            .copy_from_slice(input_bytes);

        // Instantiate
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmError::Runtime(e.to_string()))?;

        // Try to call exported function
        let result: std::result::Result<(), WasmError> =
            if let Some(func) = instance.get_func(&mut store, &self.manifest.entry_point) {
                func.call(&mut store, &[], &mut [])
                    .map_err(|e| WasmError::Trap(e.to_string()))?;
                Ok(())
            } else if let Some(func) = instance.get_func(&mut store, "_start") {
                func.call(&mut store, &[], &mut [])
                    .map_err(|e| WasmError::Trap(e.to_string()))?;
                Ok(())
            } else {
                // No entry point found, just instantiate
                Ok(())
            };

        let elapsed = start.elapsed().as_millis() as u64;

        if let Err(e) = result {
            return Ok(PluginResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
                execution_time_ms: elapsed,
                memory_used_bytes: memory.data(&store).len(),
            });
        }

        if elapsed > self.sandbox.time_limit_ms {
            return Ok(PluginResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "execution exceeded time limit: {elapsed}ms > {}ms",
                    self.sandbox.time_limit_ms
                )),
                execution_time_ms: elapsed,
                memory_used_bytes: memory.data(&store).len(),
            });
        }

        Ok(PluginResult {
            success: true,
            output: format!(
                "plugin '{}' executed with input: {} ({}ms)",
                self.manifest.name, input, elapsed
            ),
            error: None,
            execution_time_ms: elapsed,
            memory_used_bytes: memory.data(&store).len(),
        })
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn sandbox(&self) -> &SandboxConfig {
        &self.sandbox
    }
}

// ─── Plugin Registry ─────────────────────────────────────────────────────────

pub struct PluginRegistry {
    plugins: HashMap<String, WasmPlugin>,
    base_path: PathBuf,
}

impl PluginRegistry {
    pub fn new(base_path: &Path) -> Self {
        Self {
            plugins: HashMap::new(),
            base_path: base_path.to_path_buf(),
        }
    }

    pub fn register(&mut self, plugin: WasmPlugin) {
        let name = plugin.manifest().name.clone();
        self.plugins.insert(name, plugin);
    }

    pub fn get(&self, name: &str) -> Option<&WasmPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.values().map(|p| p.manifest()).collect()
    }

    pub fn remove(&mut self, name: &str) -> Option<WasmPlugin> {
        self.plugins.remove(name)
    }

    pub async fn execute_plugin(&self, name: &str, input: &str) -> Result<PluginResult> {
        let plugin = self
            .plugins
            .get(name)
            .ok_or_else(|| WasmError::NotFound(name.into()))?;
        plugin.execute(input).await
    }

    pub async fn load_from_dir(&mut self) -> Result<usize> {
        let mut count = 0;

        if !self.base_path.exists() {
            return Ok(0);
        }

        let entries =
            std::fs::read_dir(&self.base_path).map_err(|e| WasmError::Io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| WasmError::Io(e.to_string()))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                let wasm_bytes = std::fs::read(&path).map_err(|e| WasmError::Io(e.to_string()))?;

                let manifest_path = path.with_extension("json");
                let manifest = if manifest_path.exists() {
                    let json = std::fs::read_to_string(&manifest_path)
                        .map_err(|e| WasmError::Io(e.to_string()))?;
                    serde_json::from_str(&json).map_err(|e| WasmError::Serde(e.to_string()))?
                } else {
                    PluginManifest {
                        name: path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .into(),
                        ..Default::default()
                    }
                };

                self.register(WasmPlugin::new(wasm_bytes, manifest));
                count += 1;
            }
        }

        Ok(count)
    }
}

// ─── Helper: Validate WASM ──────────────────────────────────────────────────

pub fn validate_wasm(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 4 {
        return Err(WasmError::Compilation("wasm too small (< 4 bytes)".into()));
    }

    if &bytes[..4] != b"\0asm" {
        return Err(WasmError::Compilation(
            "invalid wasm magic number (expected 0x00 0x61 0x73 0x6D)".into(),
        ));
    }

    if bytes.len() < 8 {
        return Err(WasmError::Compilation("wasm header incomplete".into()));
    }

    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 1 {
        return Err(WasmError::Compilation(format!(
            "unsupported wasm version: {version}"
        )));
    }

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_wasm() {
        let mut valid_wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        valid_wasm.extend_from_slice(&[0u8; 100]); // pad
        assert!(validate_wasm(&valid_wasm).is_ok());
    }

    #[test]
    fn test_validate_wasm_invalid_magic() {
        let bad = vec![0x00, 0x00, 0x00, 0x00];
        assert!(validate_wasm(&bad).is_err());
    }

    #[test]
    fn test_validate_wasm_too_small() {
        let tiny = vec![0x00, 0x61];
        assert!(validate_wasm(&tiny).is_err());
    }

    #[test]
    fn test_plugin_manifest() {
        let m = PluginManifest {
            name: "test".into(),
            permissions: vec!["read".into()],
            memory_limit_mb: 32,
            time_limit_ms: 1000,
            ..Default::default()
        };
        assert_eq!(m.name, "test");
        assert_eq!(m.memory_limit_mb, 32);
    }

    #[test]
    fn test_sandbox_config() {
        let s = SandboxConfig::default();
        assert_eq!(s.memory_limit_mb, 64);
        assert!(s.allowed_syscalls.contains(&"proc_exit".into()));
    }

    #[test]
    fn test_plugin_result() {
        let r = PluginResult {
            success: true,
            output: "ok".into(),
            error: None,
            execution_time_ms: 42,
            memory_used_bytes: 1024,
        };
        assert!(r.success);
    }

    #[test]
    fn test_wasm_plugin_compile() {
        let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        wasm.extend_from_slice(&[0u8; 100]);
        let plugin = WasmPlugin::new(wasm, PluginManifest::default());
        assert!(plugin.compile().is_ok());
    }

    #[test]
    fn test_wasm_plugin_compile_invalid() {
        let plugin = WasmPlugin::new(vec![0, 1, 2, 3], PluginManifest::default());
        assert!(plugin.compile().is_err());
    }

    #[test]
    fn test_plugin_registry() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = PluginRegistry::new(dir.path());
        let plugin = WasmPlugin::new(vec![], PluginManifest::default());
        reg.register(plugin);
        assert!(reg.get("unknown").is_some());
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn test_plugin_registry_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = PluginRegistry::new(dir.path());
        reg.register(WasmPlugin::new(
            vec![],
            PluginManifest {
                name: "to-remove".into(),
                ..Default::default()
            },
        ));
        assert!(reg.remove("to-remove").is_some());
        assert!(reg.get("to-remove").is_none());
    }

    #[test]
    fn test_plugin_from_bytes() {
        let mut wasm = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        wasm.extend_from_slice(&[0u8; 100]);
        let m = PluginManifest {
            name: "my-plugin".into(),
            memory_limit_mb: 128,
            time_limit_ms: 10000,
            ..Default::default()
        };
        let p = WasmPlugin::new(wasm, m);
        assert_eq!(p.manifest().name, "my-plugin");
        assert_eq!(p.sandbox().memory_limit_mb, 128);
    }
}
