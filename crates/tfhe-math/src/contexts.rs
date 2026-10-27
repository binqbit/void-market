use crate::tfhe::{ciphertext, key_scope};
use crate::{ClientKey, EvaluationKey, KeyBinding, PublicKey, PublicValue, Result, TfheValue};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct EncryptionContext {
    pub(crate) key: PublicKey,
}

#[derive(Clone, Debug)]
pub struct EvaluationContext {
    pub(crate) key: EvaluationKey,
}

#[derive(Clone)]
pub struct DecryptionContext {
    pub(crate) native: Arc<::tfhe::ClientKey>,
    pub(crate) binding: KeyBinding,
}

impl std::fmt::Debug for DecryptionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecryptionContext")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

impl EncryptionContext {
    pub fn new(key: PublicKey) -> Self {
        Self { key }
    }

    pub fn binding(&self) -> KeyBinding {
        self.key.binding
    }

    pub fn encrypt(&self, value: PublicValue) -> Result<TfheValue> {
        key_scope::without_server_key(|| {
            let native = ciphertext::encrypt(&self.key.native, value.to_native()?)?;

            Ok(TfheValue::from_encrypted(native, self.binding()))
        })
    }
}

impl EvaluationContext {
    pub fn new(key: EvaluationKey) -> Self {
        Self { key }
    }

    pub fn binding(&self) -> KeyBinding {
        self.key.binding
    }
}

impl DecryptionContext {
    pub fn new(key: ClientKey) -> Self {
        Self {
            native: key.native,
            binding: key.binding,
        }
    }

    pub fn binding(&self) -> KeyBinding {
        self.binding
    }

    /// Local owner primitive. Protocol authorization/reveal gates belong above this SDK.
    pub fn decrypt(&self, value: &TfheValue) -> Result<PublicValue> {
        let native = value.encrypted_native(&self.binding)?;

        key_scope::without_server_key(|| {
            PublicValue::from_native(ciphertext::decrypt(&self.native, native))
        })
    }
}
