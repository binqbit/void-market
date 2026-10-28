use crate::http::error::{ApiError, Result};
use rand::{TryRngCore, rngs::OsRng};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hmac;
use uuid::Uuid;
use zeroize::Zeroizing;

const HEADER: &[u8; 8] = b"CUSTODY1";

pub struct Vault {
    key: LessSafeKey,
    fingerprints: hmac::Key,
}

impl Vault {
    pub fn new(key: &[u8; 32]) -> Result<Self> {
        let root = hmac::Key::new(hmac::HMAC_SHA256, key);
        let derived = hmac::sign(&root, b"CUSTODY_REQUEST_KEY_V1");
        let fingerprints = hmac::Key::new(hmac::HMAC_SHA256, derived.as_ref());
        let key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| ApiError::Internal)?;
        Ok(Self {
            key: LessSafeKey::new(key),
            fingerprints,
        })
    }

    /// A keyed request fingerprint cannot enumerate a small secret amount from a database dump.
    pub fn fingerprint(&self, owner: Uuid, purpose: &str, bytes: &[u8]) -> [u8; 32] {
        let mut context = hmac::Context::with_key(&self.fingerprints);
        context.update(&aad(owner, purpose));
        context.update(&[0]);
        context.update(bytes);
        let tag = context.sign();
        let mut fingerprint = [0; 32];
        fingerprint.copy_from_slice(tag.as_ref());
        fingerprint
    }

    pub fn seal(&self, owner: Uuid, purpose: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| ApiError::Unavailable)?;
        let mut ciphertext = bytes.to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(owner, purpose)),
                &mut ciphertext,
            )
            .map_err(|_| ApiError::Internal)?;
        let mut sealed = Vec::with_capacity(20 + ciphertext.len());
        sealed.extend_from_slice(HEADER);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    pub fn open(&self, owner: Uuid, purpose: &str, bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if bytes.len() < 36 || &bytes[..8] != HEADER {
            return Err(ApiError::Conflict("Custody record cannot be opened"));
        }
        let nonce: [u8; 12] = bytes[8..20].try_into().map_err(|_| ApiError::Internal)?;
        let mut buffer = Zeroizing::new(bytes[20..].to_vec());
        let size = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(aad(owner, purpose)),
                &mut buffer,
            )
            .map_err(|_| ApiError::Conflict("Custody record cannot be opened"))?
            .len();
        buffer.truncate(size);
        Ok(buffer)
    }
}

fn aad(owner: Uuid, purpose: &str) -> Vec<u8> {
    let mut aad = b"CUSTODY_RECORD_V1\0".to_vec();
    aad.extend_from_slice(owner.as_bytes());
    aad.extend_from_slice(purpose.as_bytes());
    aad
}
