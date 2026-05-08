//! Kernel launch functions — FFI to compiled CUDA .cu files.

use crate::memory::GpuTensor;
use crate::stream::CudaStream;
use crate::types::CutlassError;

#[cfg(feature = "cutlass")]
extern "C" {
    fn launch_cutlass_cosine_sim(
        queries: cust::sys::CUdeviceptr,
        database: cust::sys::CUdeviceptr,
        output: cust::sys::CUdeviceptr,
        num_queries: u32,
        num_db: u32,
        dim: u32,
        stream: cust::sys::CUstream,
    ) -> i32;

    fn launch_cutlass_topk(
        similarities: cust::sys::CUdeviceptr,
        values_out: cust::sys::CUdeviceptr,
        indices_out: cust::sys::CUdeviceptr,
        batch_size: u32,
        elements_per_batch: u32,
        k: u32,
        stream: cust::sys::CUstream,
    ) -> i32;

    fn launch_hnn_verlet_step(
        q: cust::sys::CUdeviceptr,
        p: cust::sys::CUdeviceptr,
        masses: cust::sys::CUdeviceptr,
        alpha: f32,
        beta: f32,
        dt: f32,
        num_trajectories: u32,
        dim: u32,
        stream: cust::sys::CUstream,
    ) -> i32;
}

/// Batch cosine similarity via cuBLAS GEMM on GPU.
pub fn cosine_sim_batch(
    queries: &GpuTensor<f32>,
    database: &GpuTensor<f32>,
    stream: &CudaStream,
) -> Result<GpuTensor<f32>, CutlassError> {
    #[cfg(not(feature = "cutlass"))]
    {
        let _ = (queries, database, stream);
        return Err(CutlassError::FeatureDisabled);
    }

    #[cfg(feature = "cutlass")]
    {
        let num_queries = queries.shape[0];
        let dim = queries.shape[1];
        let num_db = database.shape[0];

        let mut output = GpuTensor::zeros_f32(&[num_queries, num_db])?;

        let status = unsafe {
            launch_cutlass_cosine_sim(
                queries.device_ptr(),
                database.device_ptr(),
                output.device_ptr(),
                num_queries as u32,
                num_db as u32,
                dim as u32,
                stream.inner.as_inner(),
            )
        };

        if status != 0 {
            return Err(CutlassError::Kernel(format!("Cosine similarity GEMM failed (status={})", status)));
        }
        Ok(output)
    }
}

/// Top-K selection on GPU.
pub fn topk(
    similarities: &GpuTensor<f32>,
    k: u32,
    stream: &CudaStream,
) -> Result<(GpuTensor<f32>, GpuTensor<u32>), CutlassError> {
    #[cfg(not(feature = "cutlass"))]
    {
        let _ = (similarities, k, stream);
        return Err(CutlassError::FeatureDisabled);
    }

    #[cfg(feature = "cutlass")]
    {
        let batch_size = similarities.shape[0];
        let elements = similarities.shape[1];

        let mut values_out = GpuTensor::zeros_f32(&[batch_size, k as usize])?;
        let mut indices_out = GpuTensor::zeros_u32(&[batch_size, k as usize])?;

        let status = unsafe {
            launch_cutlass_topk(
                similarities.device_ptr(),
                values_out.device_ptr(),
                indices_out.device_ptr(),
                batch_size as u32,
                elements as u32,
                k,
                stream.inner.as_inner(),
            )
        };

        if status != 0 {
            return Err(CutlassError::Kernel("Top-K extraction failed".into()));
        }
        Ok((values_out, indices_out))
    }
}

/// Hamiltonian Neural Network Verlet integration step on GPU.
pub fn hnn_verlet_step(
    q: &mut GpuTensor<f32>,
    p: &mut GpuTensor<f32>,
    masses: &GpuTensor<f32>,
    alpha: f32,
    beta: f32,
    dt: f32,
    stream: &CudaStream,
) -> Result<(), CutlassError> {
    #[cfg(not(feature = "cutlass"))]
    {
        let _ = (q, p, masses, alpha, beta, dt, stream);
        return Err(CutlassError::FeatureDisabled);
    }

    #[cfg(feature = "cutlass")]
    {
        let num_trajectories = q.shape[0];
        let dim = q.shape[1];

        let status = unsafe {
            launch_hnn_verlet_step(
                q.device_ptr(),
                p.device_ptr(),
                masses.device_ptr(),
                alpha,
                beta,
                dt,
                num_trajectories as u32,
                dim as u32,
                stream.inner.as_inner(),
            )
        };

        if status != 0 {
            return Err(CutlassError::Kernel("HNN Verlet integration failed".into()));
        }
        Ok(())
    }
}