#[cfg(feature = "tfhe-backend")]
use crate::tfhe::value::NativeValue;
use crate::{DataType, PublicValue, Result, Visibility};
#[cfg(feature = "tfhe-backend")]
use crate::{KeyBinding, MathError};

#[derive(Clone)]
pub(crate) enum Storage {
    Public(PublicValue),
    #[cfg(feature = "tfhe-backend")]
    Encrypted(Box<EncryptedValue>),
}

#[cfg(feature = "tfhe-backend")]
#[derive(Clone)]
pub(crate) struct EncryptedValue {
    pub native: NativeValue,
    pub binding: KeyBinding,
}

/// Opaque math value. Debug deliberately omits plaintext and ciphertext bodies.
#[derive(Clone)]
pub struct TfheValue {
    pub(crate) storage: Storage,
}

impl TfheValue {
    #[cfg(feature = "tfhe-backend")]
    pub(crate) fn from_encrypted(native: NativeValue, binding: KeyBinding) -> Self {
        Self {
            storage: Storage::Encrypted(Box::new(EncryptedValue { native, binding })),
        }
    }

    #[cfg(feature = "tfhe-backend")]
    pub(crate) fn encrypted_native(&self, binding: &KeyBinding) -> Result<&NativeValue> {
        let Storage::Encrypted(value) = &self.storage else {
            return Err(MathError::BackendMismatch);
        };

        if value.binding.config != binding.config {
            return Err(MathError::ConfigMismatch);
        }
        if value.binding != *binding {
            return Err(MathError::KeyDomainMismatch);
        }

        Ok(&value.native)
    }

    pub const fn public(value: PublicValue) -> Self {
        Self {
            storage: Storage::Public(value),
        }
    }

    pub fn data_type(&self) -> DataType {
        match &self.storage {
            Storage::Public(v) => v.data_type(),
            #[cfg(feature = "tfhe-backend")]
            Storage::Encrypted(v) => v.native.data_type(),
        }
    }

    pub fn visibility(&self) -> Visibility {
        match self.storage {
            Storage::Public(_) => Visibility::Public,
            #[cfg(feature = "tfhe-backend")]
            Storage::Encrypted(_) => Visibility::Encrypted,
        }
    }

    pub fn public_value(&self) -> Result<PublicValue> {
        match &self.storage {
            Storage::Public(v) => Ok(*v),
            #[cfg(feature = "tfhe-backend")]
            Storage::Encrypted(_) => Err(MathError::BackendMismatch),
        }
    }
}

impl From<PublicValue> for TfheValue {
    fn from(value: PublicValue) -> Self {
        Self::public(value)
    }
}

impl std::fmt::Debug for TfheValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TfheValue")
            .field("data_type", &self.data_type())
            .field("visibility", &self.visibility())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct FlaggedValue {
    /// Modular candidate in the explicitly requested width.
    pub value: TfheValue,
    /// Boolean overflow, or borrow for subtraction; encrypted whenever an
    /// operand is encrypted. Inspecting it requires the decryption capability.
    pub flag: TfheValue,
}
