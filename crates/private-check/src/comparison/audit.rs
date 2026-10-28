use super::answer::{AnswerCommitment, MaskedBooleanAnswer};
use super::evaluate::{DecodedBoolean, PreparedComparison};
use super::owner_masks::OwnerMaskOpening;
use crate::checks::{boolean, commitments};
use crate::{CheckContext, CheckError, KeyOwnerContext, Result};
use zeroize::Zeroizing;

pub(crate) struct AuditMaterial {
    pub prepared: PreparedComparison,
    pub commitment: AnswerCommitment,
    pub answer: MaskedBooleanAnswer,
    pub openings: [OwnerMaskOpening; 2],
}

/// Sensitive Boolean-only audit material. Numeric transfer witnesses are absent.
pub struct BooleanAuditOpening {
    pub(crate) material: AuditMaterial,
    pub(crate) decoded: bool,
}

impl DecodedBoolean {
    /// Explicit opening primitive for a host-authorized final auditor who may learn the bit.
    /// This method does not infer payment, finalization or audience authorization.
    pub fn into_audit_opening(self) -> BooleanAuditOpening {
        BooleanAuditOpening {
            material: self.audit,
            decoded: self.value,
        }
    }
}

impl KeyOwnerContext {
    /// Recheck the exact original encrypted Boolean masks and mathematical answer.
    /// This is a local-key audit, not a public zero-knowledge proof.
    pub fn check_boolean_audit(
        &self,
        opening: &BooleanAuditOpening,
        expected: &CheckContext,
    ) -> Result<bool> {
        if self.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }
        let material = &opening.material;
        let prepared = &material.prepared;
        expected.check_id(prepared.masks.context)?;
        let encrypted = &prepared.masks.encrypted;
        let base = commitments::base(
            expected.id,
            expected.width(),
            [
                encrypted[0].wire.native_payload(),
                encrypted[1].wire.native_payload(),
                encrypted[2].wire.native_payload(),
            ],
        )?;
        if base != prepared.masks.commitment || base != prepared.challenge.binding.base {
            return Err(CheckError::CommitmentMismatch);
        }

        let masks = &prepared.masks.masks;
        for (ciphertext, value) in encrypted
            .iter()
            .zip([&masks.spacing, &masks.offset, &masks.pad])
        {
            if self.decryption.decrypt(&ciphertext.value)?.as_uint()? != **value {
                return Err(CheckError::CommitmentMismatch);
            }
        }

        let actual_answer = Zeroizing::new(
            self.decryption
                .decrypt(&prepared.challenge.artifact.value)?
                .as_uint()?,
        );
        if *actual_answer != *material.answer.value {
            return Err(CheckError::InvalidBooleanAnswer);
        }

        let decoded = boolean::decode(
            prepared,
            material.commitment,
            &material.answer,
            [&material.openings[0], &material.openings[1]],
            expected,
        )?;
        if decoded != opening.decoded {
            return Err(CheckError::InvalidBooleanAnswer);
        }

        Ok(decoded)
    }
}
