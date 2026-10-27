#[cfg(feature = "tfhe-backend")]
use sha2::{Digest, Sha256};

/// The full, pinned CPU configuration. Protocol widths do not change HE parameters.
#[derive(Debug, Clone, Copy, Default)]
pub struct MathParameters;

impl MathParameters {
    pub const BACKEND_VERSION: &'static str = "tfhe-rs-1.8.2";

    #[cfg(feature = "tfhe-backend")]
    pub fn fingerprint(self) -> crate::Result<[u8; 32]> {
        let config = crate::tfhe::parameters::canonical_config_bytes()?;

        let mut h = Sha256::new();
        h.update(b"TFHE_MATH_CONFIG_V1\0");
        h.update(Self::BACKEND_VERSION.as_bytes());
        h.update(b"\0");
        h.update(config);

        Ok(h.finalize().into())
    }
}
