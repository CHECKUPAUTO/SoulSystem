//! CUDA stream wrapper for async kernel execution.

use crate::types::CutlassError;

pub struct CudaStream {
    #[cfg(feature = "cutlass")]
    pub(crate) inner: cust::stream::Stream,
}

impl CudaStream {
    pub fn new() -> Result<Self, CutlassError> {
        #[cfg(not(feature = "cutlass"))]
        return Err(CutlassError::FeatureDisabled);

        #[cfg(feature = "cutlass")]
        {
            let inner = cust::stream::Stream::new(cust::stream::StreamFlags::NON_BLOCKING, None)?;
            Ok(Self { inner })
        }
    }

    pub fn synchronize(&self) -> Result<(), CutlassError> {
        #[cfg(not(feature = "cutlass"))]
        return Err(CutlassError::FeatureDisabled);

        #[cfg(feature = "cutlass")]
        {
            self.inner.synchronize()?;
            Ok(())
        }
    }
}
