//! DALI Pipeline — FFI bindings with auto-detection via cfg(feature = "dali").

use crate::types::{DaliError, Device};

// ── cxx bridge (always compiled, but only linked when dali cfg is set) ─────

#[cfg(feature = "dali")]
#[cxx::bridge]
mod dali_ffi {
    unsafe extern "C++" {
        include!("soullink-dali/cxx-bridge/dali_wrapper.h");

        type DaliPipelineHandle;

        fn dali_create_pipeline(
            max_batch_size: i32,
            num_threads: i32,
            device_id: i32,
        ) -> Result<UniquePtr<DaliPipelineHandle>>;

        fn add_operator(self: &DaliPipelineHandle, serialized_spec: &str, inst_name: &str) -> bool;
        fn build(self: &DaliPipelineHandle) -> bool;
        fn run(self: &DaliPipelineHandle) -> bool;
        fn output_count(self: &DaliPipelineHandle) -> usize;
        fn output_dtype(self: &DaliPipelineHandle, idx: usize) -> u32;
        fn output_device(self: &DaliPipelineHandle, idx: usize) -> i32;
        fn copy_output_to_host(self: &DaliPipelineHandle, idx: usize, dst: &mut [f32]) -> usize;
    }
}

// ── DaliPipeline ───────────────────────────────────────────────────────────

pub struct DaliPipeline {
    #[cfg(feature = "dali")]
    handle: cxx::UniquePtr<dali_ffi::DaliPipelineHandle>,
    batch_size: u32,
    num_threads: u32,
    device: Device,
    built: bool,
}

// ── Builder ────────────────────────────────────────────────────────────────

pub struct DaliPipelineBuilder {
    batch_size: u32,
    num_threads: u32,
    device: Device,
    ops: Vec<(String, String)>,
}

impl DaliPipelineBuilder {
    pub fn new() -> Self {
        Self {
            batch_size: 1,
            num_threads: 4,
            device: Device::Cpu,
            ops: Vec::new(),
        }
    }

    pub fn batch_size(mut self, size: u32) -> Self {
        self.batch_size = size;
        self
    }
    pub fn num_threads(mut self, threads: u32) -> Self {
        self.num_threads = threads;
        self
    }
    pub fn device(mut self, device: Device) -> Self {
        self.device = device;
        self
    }

    pub fn add_op(mut self, spec: impl Into<String>, name: impl Into<String>) -> Self {
        self.ops.push((spec.into(), name.into()));
        self
    }

    pub fn build(self) -> Result<DaliPipeline, DaliError> {
        #[cfg(not(feature = "dali"))]
        {
            let _ = (self.batch_size, self.num_threads, self.device, self.ops);
            return Err(DaliError::FeatureDisabled);
        }

        #[cfg(feature = "dali")]
        {
            let device_id = match self.device {
                Device::Gpu(id) => id,
                _ => 0,
            };

            let handle = dali_ffi::dali_create_pipeline(
                self.batch_size as i32,
                self.num_threads as i32,
                device_id,
            )
            .map_err(|e| DaliError::Init(e.to_string()))?;

            let mut pipeline = DaliPipeline {
                handle,
                batch_size: self.batch_size,
                num_threads: self.num_threads,
                device: self.device,
                built: false,
            };

            for (spec, name) in &self.ops {
                if !pipeline.handle.add_operator(spec, name) {
                    return Err(DaliError::Build(format!(
                        "Failed to add operator: {}",
                        name
                    )));
                }
            }

            if !pipeline.handle.build() {
                return Err(DaliError::Build("Pipeline build failed".into()));
            }
            pipeline.built = true;

            Ok(pipeline)
        }
    }
}

impl Default for DaliPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DaliPipeline {
    pub fn builder() -> DaliPipelineBuilder {
        DaliPipelineBuilder::new()
    }
    pub fn batch_size(&self) -> u32 {
        self.batch_size
    }

    pub fn run(&self) -> Result<(), DaliError> {
        #[cfg(not(feature = "dali"))]
        return Err(DaliError::FeatureDisabled);

        #[cfg(feature = "dali")]
        {
            if !self.built {
                return Err(DaliError::Run("Pipeline not built".into()));
            }
            if !self.handle.run() {
                return Err(DaliError::Run("Pipeline run failed".into()));
            }
            Ok(())
        }
    }

    pub fn output_count(&self) -> usize {
        #[cfg(not(feature = "dali"))]
        return 0;

        #[cfg(feature = "dali")]
        self.handle.output_count()
    }

    pub fn output_to_host(&self, idx: usize, dst: &mut [f32]) -> Result<usize, DaliError> {
        #[cfg(not(feature = "dali"))]
        {
            let _ = (idx, dst);
            return Err(DaliError::FeatureDisabled);
        }

        #[cfg(feature = "dali")]
        {
            let n = self.handle.copy_output_to_host(idx, dst);
            if n == 0 {
                return Err(DaliError::Run("copy_output_to_host returned 0".into()));
            }
            Ok(n)
        }
    }
}
