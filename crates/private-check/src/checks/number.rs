use crate::values::CiphertextArtifact;
use crate::{CheckError, CheckSession, Digest, NumberInput, Result};
use tfhe_math::{KeyBinding, PublicValue, Width, codec::EncodedObject};

pub fn validate_public_number(width: Width, value: u64) -> Result<()> {
    if value > width.maximum() / 2 {
        return Err(CheckError::NumberOutOfRange);
    }
    Ok(())
}

/// Fixed input-domain predicate. Only the key owner may decrypt its Boolean value.
pub struct EncryptedNumberCheck {
    pub(crate) context: Digest,
    pub(crate) input: Digest,
    pub(crate) key: KeyBinding,
    pub(crate) artifact: CiphertextArtifact,
}

impl EncryptedNumberCheck {
    pub fn context(&self) -> Digest {
        self.context
    }

    pub fn input_digest(&self) -> Digest {
        self.input
    }

    pub fn key_binding(&self) -> KeyBinding {
        self.key
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.artifact.wire
    }
}

impl CheckSession {
    pub fn check_number(&self, input: &NumberInput) -> Result<EncryptedNumberCheck> {
        let domain = if input.binding == self.context.source {
            &self.source
        } else if input.binding == self.context.target {
            &self.target
        } else {
            return Err(CheckError::ReferenceMismatch);
        };
        let limit = PublicValue::uint(self.context.width(), self.context.width().maximum() / 2)?;
        let value = domain.math().le(&input.value, &limit.into())?;

        Ok(EncryptedNumberCheck {
            context: self.context.id,
            input: input.binding.native,
            key: domain.binding(),
            artifact: CiphertextArtifact::from_value(value)?,
        })
    }
}
