//! Owner-only key persistence. These bytes are secret, not transport artifacts.
//! Authenticate and decrypt local storage before importing; never accept these
//! records as remote key material or include them in an API response.
use crate::{ClientKey, KeyBinding, MathError, Result, tfhe::serialization};
use std::sync::Arc;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"TFHEKEY1";
const HEADER: usize = 136;
pub const CLIENT_KEY_LIMIT: u64 = 16 * 1024 * 1024;

pub fn export_client_key(key: &ClientKey) -> Result<Zeroizing<Vec<u8>>> {
    let native = Zeroizing::new(serialization::serialize(
        &*key.native,
        CLIENT_KEY_LIMIT - HEADER as u64,
    )?);
    let mut bytes = Zeroizing::new(Vec::with_capacity(HEADER + native.len()));
    bytes.extend_from_slice(MAGIC);
    for fingerprint in [
        key.binding.config,
        key.binding.public,
        key.binding.evaluation,
        key.binding.domain,
    ] {
        bytes.extend_from_slice(&fingerprint);
    }
    bytes.extend_from_slice(&native);
    Ok(bytes)
}

pub fn import_client_key(bytes: &[u8], binding: KeyBinding) -> Result<ClientKey> {
    if bytes.len() as u64 > CLIENT_KEY_LIMIT {
        return Err(MathError::ResourceLimit);
    }
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(MathError::InvalidEncoding);
    }
    if bytes[8..40] != binding.config
        || bytes[40..72] != binding.public
        || bytes[72..104] != binding.evaluation
        || bytes[104..136] != binding.domain
    {
        return Err(MathError::KeyDomainMismatch);
    }
    let native =
        serialization::deserialize_client_key(&bytes[HEADER..], CLIENT_KEY_LIMIT - HEADER as u64)?;
    Ok(ClientKey {
        native: Arc::new(native),
        binding,
    })
}
