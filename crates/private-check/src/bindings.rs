use crate::checks::commitments;
use crate::{CheckError, NumberInput, Result, random};
use sha2::{Digest as HashDigest, Sha256};
use tfhe_math::{KeyBinding, Width, codec::EncodedObject};

pub type Digest = [u8; 32];

/// Existing canonical UUIDv4 owner bytes; identity authentication belongs to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerId([u8; 16]);

impl OwnerId {
    pub fn new(bytes: [u8; 16]) -> Result<Self> {
        if bytes[6] >> 4 != 4 || bytes[8] >> 6 != 2 {
            return Err(CheckError::InvalidOwner);
        }

        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Comparison {
    Equal = 1,
    NotEqual = 2,
    Less = 3,
    LessOrEqual = 4,
    Greater = 5,
    GreaterOrEqual = 6,
}

/// Full-frame and native-payload identities are deliberately distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputBinding {
    pub(crate) frame: Digest,
    pub(crate) native: Digest,
    pub(crate) key: KeyBinding,
    pub(crate) width: Width,
}

impl InputBinding {
    pub(crate) fn new(wire: &EncodedObject, key: KeyBinding, width: Width) -> Self {
        Self {
            frame: wire.digest(),
            native: Sha256::digest(wire.native_payload()).into(),
            key,
            width,
        }
    }

    /// Reconstruct public metadata from the caller's pinned original input record.
    /// This constructor establishes neither ciphertext correctness nor authentication.
    pub fn from_parts(frame: Digest, native: Digest, key: KeyBinding, width: Width) -> Self {
        Self {
            frame,
            native,
            key,
            width,
        }
    }

    pub fn frame_digest(&self) -> Digest {
        self.frame
    }

    pub fn native_digest(&self) -> Digest {
        self.native
    }

    pub fn key_binding(&self) -> KeyBinding {
        self.key
    }

    pub fn width(&self) -> Width {
        self.width
    }
}

/// Immutable mathematical operation binding; not an authorization or proof receipt.
#[derive(Debug, Clone)]
pub struct CheckContext {
    pub(crate) external: Digest,
    pub(crate) nonce: Digest,
    pub(crate) owners: [OwnerId; 2],
    pub(crate) comparison: Comparison,
    pub(crate) source: InputBinding,
    pub(crate) target: InputBinding,
    pub(crate) id: Digest,
}

impl CheckContext {
    pub fn new(
        external_context: Digest,
        owners: [OwnerId; 2],
        comparison: Comparison,
        source: &NumberInput,
        target: &NumberInput,
    ) -> Result<Self> {
        Self::from_parts(
            external_context,
            *random::bytes::<32>()?,
            owners,
            comparison,
            source.binding,
            target.binding,
        )
    }

    /// Restore only the public operation binding, never ephemeral masks or authority.
    /// Every recipient pins these fields independently of the incoming challenge.
    pub fn from_parts(
        external_context: Digest,
        nonce: Digest,
        owners: [OwnerId; 2],
        comparison: Comparison,
        source: InputBinding,
        target: InputBinding,
    ) -> Result<Self> {
        if owners[0] == owners[1] {
            return Err(CheckError::InvalidOwner);
        }
        if source.width != target.width {
            return Err(CheckError::TypeMismatch);
        }
        if source.key.config_fingerprint() != target.key.config_fingerprint() {
            return Err(CheckError::Math(tfhe_math::MathError::ConfigMismatch));
        }
        if source.key == target.key {
            return Err(CheckError::SameKeyDomain);
        }

        let mut context = Self {
            external: external_context,
            nonce,
            owners,
            comparison,
            source,
            target,
            id: [0; 32],
        };
        context.id = commitments::context_digest(&context)?;

        Ok(context)
    }

    pub fn id(&self) -> Digest {
        self.id
    }

    pub fn nonce(&self) -> Digest {
        self.nonce
    }

    pub fn external_context(&self) -> Digest {
        self.external
    }

    pub fn width(&self) -> Width {
        self.source.width
    }

    pub fn comparison(&self) -> Comparison {
        self.comparison
    }

    pub fn owners(&self) -> [OwnerId; 2] {
        self.owners
    }

    pub fn source(&self) -> InputBinding {
        self.source
    }

    pub fn target(&self) -> InputBinding {
        self.target
    }

    pub(crate) fn check_id(&self, actual: Digest) -> Result<()> {
        if self.id != actual {
            return Err(CheckError::ContextMismatch);
        }
        Ok(())
    }

    pub(crate) fn check_owner(&self, owner: OwnerId) -> Result<()> {
        if !self.owners.contains(&owner) {
            return Err(CheckError::InvalidOwner);
        }
        Ok(())
    }
}
