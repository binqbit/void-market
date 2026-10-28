use crate::{CheckError, Result};
use tfhe_math::{DecryptionContext, EncryptionContext, EvaluationContext, KeyBinding, MathSession};

/// Public encryption/evaluation material for one domain; contains no secret key.
#[derive(Clone, Debug)]
pub struct ComputationContext {
    pub(crate) encryption: EncryptionContext,
    pub(crate) evaluation: EvaluationContext,
}

impl ComputationContext {
    pub fn new(encryption: EncryptionContext, evaluation: EvaluationContext) -> Result<Self> {
        if encryption.binding() != evaluation.binding() {
            return Err(CheckError::KeyDomainMismatch);
        }

        Ok(Self {
            encryption,
            evaluation,
        })
    }

    pub fn binding(&self) -> KeyBinding {
        self.evaluation.binding()
    }

    pub(crate) fn math(&self) -> MathSession {
        MathSession::encrypted(self.evaluation.clone())
    }
}

/// Local key-owner capability. Public evaluation material validates native imports.
pub struct KeyOwnerContext {
    pub(crate) decryption: DecryptionContext,
    pub(crate) evaluation: EvaluationContext,
}

impl KeyOwnerContext {
    pub fn new(decryption: DecryptionContext, evaluation: EvaluationContext) -> Result<Self> {
        if decryption.binding() != evaluation.binding() {
            return Err(CheckError::KeyDomainMismatch);
        }

        Ok(Self {
            decryption,
            evaluation,
        })
    }

    pub fn binding(&self) -> KeyBinding {
        self.decryption.binding()
    }
}

impl std::fmt::Debug for KeyOwnerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyOwnerContext")
            .field("binding", &self.binding())
            .finish_non_exhaustive()
    }
}
