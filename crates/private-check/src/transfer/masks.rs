use crate::{Result, random};
use tfhe_math::Width;
use zeroize::Zeroizing;

/// No persistence, opening or cloning operation exists for numeric witnesses.
pub(super) struct NumericMasks {
    pub offset: Zeroizing<u64>,
    pub pad: Zeroizing<u64>,
}

impl NumericMasks {
    pub fn generate(width: Width) -> Result<Self> {
        Ok(Self {
            offset: random::bounded(width, false)?,
            pad: random::scalar(width)?,
        })
    }
}
