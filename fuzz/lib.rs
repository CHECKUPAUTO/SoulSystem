//! Shared fuzz helpers for SoulSystem fuzz targets.

use arbitrary::Arbitrary;

/// A byte buffer that derives Arbitrary for raw-data fuzzing.
#[derive(Arbitrary, Debug)]
pub struct FuzzData {
    pub data: Vec<u8>,
}

impl FuzzData {
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }
}
