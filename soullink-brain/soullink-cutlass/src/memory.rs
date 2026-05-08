//! GPU tensor wrapper with shape tracking. Uses cust for memory management.

use crate::types::CutlassError;

#[cfg(feature = "cutlass")]
use cust::memory::{DeviceBuffer, CopyDestination};

/// A tensor living in GPU memory.
#[cfg(feature = "cutlass")]
pub struct GpuTensor<T: cust::memory::DeviceCopy> {
    pub(crate) buffer: DeviceBuffer<T>,
    pub shape: Vec<usize>,
    _marker: std::marker::PhantomData<T>,
}

#[cfg(not(feature = "cutlass"))]
pub struct GpuTensor<T> {
    pub shape: Vec<usize>,
    _marker: std::marker::PhantomData<T>,
}

// ── f32 implementation ──────────────────────────────────────────────────────

#[cfg(feature = "cutlass")]
impl GpuTensor<f32> {
    pub fn from_slice_f32(data: &[f32], shape: &[usize]) -> Result<Self, CutlassError> {
        let expected_len: usize = shape.iter().product();
        if data.len() != expected_len {
            return Err(CutlassError::Memory(format!("Shape mismatch: {} vs {}", data.len(), expected_len)));
        }
        let buffer = DeviceBuffer::from_slice(data)?;
        Ok(Self { buffer, shape: shape.to_vec(), _marker: std::marker::PhantomData })
    }

    pub fn zeros_f32(shape: &[usize]) -> Result<Self, CutlassError> {
        let len: usize = shape.iter().product();
        let buffer = DeviceBuffer::from_slice(&vec![0.0f32; len])?;
        Ok(Self { buffer, shape: shape.to_vec(), _marker: std::marker::PhantomData })
    }

    pub fn to_vec_f32(&self) -> Result<Vec<f32>, CutlassError> {
        let mut host = vec![0.0f32; self.buffer.len()];
        self.buffer.copy_to(&mut host)?;
        Ok(host)
    }

    pub fn device_ptr(&self) -> cust::sys::CUdeviceptr {
        self.buffer.as_device_ptr().as_raw()
    }
}

#[cfg(feature = "cutlass")]
impl GpuTensor<u32> {
    pub fn zeros_u32(shape: &[usize]) -> Result<Self, CutlassError> {
        let len: usize = shape.iter().product();
        let buffer = DeviceBuffer::from_slice(&vec![0u32; len])?;
        Ok(Self { buffer, shape: shape.to_vec(), _marker: std::marker::PhantomData })
    }

    pub fn device_ptr(&self) -> cust::sys::CUdeviceptr {
        self.buffer.as_device_ptr().as_raw()
    }
}

// ── Common ─────────────────────────────────────────────────────────────────

#[cfg(feature = "cutlass")]
impl<T: cust::memory::DeviceCopy> GpuTensor<T> {
    pub fn len(&self) -> usize { self.shape.iter().product() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

#[cfg(not(feature = "cutlass"))]
impl<T> GpuTensor<T> {
    pub fn len(&self) -> usize { self.shape.iter().product() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

// ── Stubs (no cutlass feature) ─────────────────────────────────────────────

#[cfg(not(feature = "cutlass"))]
impl GpuTensor<f32> {
    pub fn from_slice_f32(_data: &[f32], _shape: &[usize]) -> Result<Self, CutlassError> {
        Err(CutlassError::FeatureDisabled)
    }
    pub fn zeros_f32(_shape: &[usize]) -> Result<Self, CutlassError> {
        Err(CutlassError::FeatureDisabled)
    }
    pub fn to_vec_f32(&self) -> Result<Vec<f32>, CutlassError> {
        Err(CutlassError::FeatureDisabled)
    }
}

#[cfg(not(feature = "cutlass"))]
impl GpuTensor<u32> {
    pub fn zeros_u32(_shape: &[usize]) -> Result<Self, CutlassError> {
        Err(CutlassError::FeatureDisabled)
    }
}