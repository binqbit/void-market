use crate::values::CiphertextArtifact;
use crate::{CheckError, CheckSession, Digest, NumberInput, Result};
use tfhe_math::{KeyBinding, MathSession, PublicValue, TfheValue, Width, codec::EncodedObject};

pub fn validate_public_number(width: Width, value: u64) -> Result<()> {
    if value > width.maximum() / 2 {
        return Err(CheckError::NumberOutOfRange);
    }
    Ok(())
}

pub(crate) fn encrypted_range_check(
    math: &MathSession,
    width: Width,
    value: &TfheValue,
) -> Result<CiphertextArtifact> {
    let limit = PublicValue::uint(width, width.maximum() / 2)?;
    CiphertextArtifact::from_value(math.le(value, &limit.into())?)
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
        Ok(EncryptedNumberCheck {
            context: self.context.id,
            input: input.binding.native,
            key: domain.binding(),
            artifact: encrypted_range_check(&domain.math(), self.context.width(), &input.value)?,
        })
    }
}
