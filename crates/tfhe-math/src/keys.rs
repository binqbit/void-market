use crate::tfhe::{key_scope, keys as native_keys, serialization};
use crate::{MathError, MathParameters, Result};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Fingerprints must come from the caller's authenticated key registry when
/// restoring remote material. Structural conformance alone is not a same-key proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub(crate) config: [u8; 32],
    pub(crate) public: [u8; 32],
    pub(crate) evaluation: [u8; 32],
    pub(crate) domain: [u8; 32],
}

impl KeyBinding {
    pub fn from_fingerprints(
        config: [u8; 32],
        public: [u8; 32],
        evaluation: [u8; 32],
    ) -> Result<Self> {
        if config != MathParameters.fingerprint()? {
            return Err(MathError::ConfigMismatch);
        }

        let mut hash = Sha256::new();
        hash.update(b"TFHE_MATH_DOMAIN_V1\0");
        hash.update(config);
        hash.update(public);
        hash.update(evaluation);

        Ok(Self {
            config,
            public,
            evaluation,
            domain: hash.finalize().into(),
        })
    }

    pub const fn domain_id(&self) -> [u8; 32] {
        self.domain
    }

    pub const fn config_fingerprint(&self) -> [u8; 32] {
        self.config
    }

    pub const fn public_key_fingerprint(&self) -> [u8; 32] {
        self.public
    }

    pub const fn evaluation_key_fingerprint(&self) -> [u8; 32] {
        self.evaluation
    }
}

pub struct ClientKey {
    pub(crate) native: Arc<::tfhe::ClientKey>,
    pub(crate) binding: KeyBinding,
}

#[derive(Clone)]
pub struct PublicKey {
    pub(crate) native: Arc<::tfhe::CompactPublicKey>,
    pub(crate) binding: KeyBinding,
}

#[derive(Clone)]
pub struct EvaluationKey {
    pub(crate) native: Arc<::tfhe::ServerKey>,
    pub(crate) binding: KeyBinding,
}

impl ClientKey {
    pub fn binding(&self) -> KeyBinding {
        self.binding
    }
}

impl PublicKey {
    pub fn binding(&self) -> KeyBinding {
        self.binding
    }
}

impl EvaluationKey {
    pub fn binding(&self) -> KeyBinding {
        self.binding
    }
}

pub struct KeySet {
    pub(crate) client: ClientKey,
    pub(crate) public: PublicKey,
    pub(crate) evaluation: EvaluationKey,
}

impl KeySet {
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    pub fn evaluation_key(&self) -> &EvaluationKey {
        &self.evaluation
    }

    pub fn generate(parameters: MathParameters) -> Result<Self> {
        key_scope::without_server_key(|| {
            let native = native_keys::generate();
            let public_bytes = serialization::serialize(&native.public, 16 * 1024 * 1024)?;
            let evaluation_bytes = serialization::serialize(&native.evaluation, 512 * 1024 * 1024)?;

            let binding = KeyBinding::from_fingerprints(
                parameters.fingerprint()?,
                Sha256::digest(&public_bytes).into(),
                Sha256::digest(&evaluation_bytes).into(),
            )?;

            Ok(Self {
                client: ClientKey {
                    native: Arc::new(native.client),
                    binding,
                },
                public: PublicKey {
                    native: Arc::new(native.public),
                    binding,
                },
                evaluation: EvaluationKey {
                    native: Arc::new(native.evaluation),
                    binding,
                },
            })
        })
    }

    pub fn binding(&self) -> KeyBinding {
        self.client.binding
    }

    pub fn encryption_context(&self) -> crate::EncryptionContext {
        crate::EncryptionContext {
            key: self.public.clone(),
        }
    }

    pub fn evaluation_context(&self) -> crate::EvaluationContext {
        crate::EvaluationContext {
            key: self.evaluation.clone(),
        }
    }

    pub fn decryption_context(&self) -> crate::DecryptionContext {
        crate::DecryptionContext {
            native: self.client.native.clone(),
            binding: self.client.binding,
        }
    }

    pub fn into_parts(self) -> (ClientKey, PublicKey, EvaluationKey) {
        (self.client, self.public, self.evaluation)
    }
}

macro_rules! redacted_debug {
    ($($t:ty),+ $(,)?) => {
        $(
            impl std::fmt::Debug for $t {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.debug_struct(stringify!($t))
                        .field("binding", &self.binding)
                        .finish_non_exhaustive()
                }
            }
        )+
    };
}
redacted_debug!(ClientKey, PublicKey, EvaluationKey);
