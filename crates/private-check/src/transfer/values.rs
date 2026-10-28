use super::masks::NumericMasks;
use crate::checks::commitments;
use crate::values::CiphertextArtifact;
use crate::{CheckContext, CheckError, Digest, EncryptedNumberCheck, InputBinding, Result};
use tfhe_math::{DataType, EvaluationContext, TfheValue, codec::EncodedObject};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferBinding {
    pub context: Digest,
    /// Original native input digest for a challenge; full challenge ID for a response.
    pub reference: Digest,
}

pub struct TransferChallenge {
    pub(crate) binding: TransferBinding,
    pub(crate) artifact: CiphertextArtifact,
    pub(crate) id: Digest,
}

impl TransferChallenge {
    pub fn binding(&self) -> TransferBinding {
        self.binding
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.artifact.wire
    }

    pub fn id(&self) -> Digest {
        self.id
    }

    pub fn import(
        binding: TransferBinding,
        bytes: &[u8],
        digest: Digest,
        expected: &CheckContext,
        source: &EvaluationContext,
    ) -> Result<Self> {
        expected.check_id(binding.context)?;
        if binding.reference != expected.source.native {
            return Err(CheckError::ReferenceMismatch);
        }
        if source.binding() != expected.source.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let artifact =
            CiphertextArtifact::import(bytes, digest, DataType::Uint(expected.width()), source)?;
        let id = commitments::transfer_reference(
            binding.context,
            binding.reference,
            artifact.wire.digest(),
        )?;
        Ok(Self {
            binding,
            artifact,
            id,
        })
    }
}

pub struct TransferResponse {
    pub(crate) binding: TransferBinding,
    pub(crate) artifact: CiphertextArtifact,
}

impl TransferResponse {
    pub fn binding(&self) -> TransferBinding {
        self.binding
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.artifact.wire
    }

    /// Validates public bindings and native shape, not the remote Dec/Enc relation.
    pub fn import(
        binding: TransferBinding,
        bytes: &[u8],
        digest: Digest,
        challenge: &TransferChallenge,
        expected: &CheckContext,
        target: &EvaluationContext,
    ) -> Result<Self> {
        expected.check_id(binding.context)?;
        expected.check_id(challenge.binding.context)?;
        if binding.reference != challenge.id {
            return Err(CheckError::ReferenceMismatch);
        }
        if target.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let artifact =
            CiphertextArtifact::import(bytes, digest, DataType::Uint(expected.width()), target)?;
        Ok(Self { binding, artifact })
    }
}

pub struct PreparedTransfer {
    pub(crate) challenge: TransferChallenge,
    pub(super) masks: NumericMasks,
    pub(crate) range: EncryptedNumberCheck,
    pub(crate) overflow: CiphertextArtifact,
}

impl PreparedTransfer {
    pub fn challenge(&self) -> &TransferChallenge {
        &self.challenge
    }

    pub fn range_check(&self) -> &EncryptedNumberCheck {
        &self.range
    }
    /// Encrypted source-domain flag; it is never turned into a host-side error.
    pub fn addition_overflow(&self) -> &EncodedObject {
        &self.overflow.wire
    }
}

/// Computation-only target-domain value. Deliberately has no export/value accessor.
pub struct TransferredNumber {
    pub(crate) context: Digest,
    pub(crate) source: InputBinding,
    pub(crate) value: TfheValue,
    pub(crate) borrow: CiphertextArtifact,
}

impl TransferredNumber {
    pub fn context(&self) -> Digest {
        self.context
    }

    pub fn source_binding(&self) -> InputBinding {
        self.source
    }

    pub fn subtraction_borrow(&self) -> &EncodedObject {
        &self.borrow.wire
    }
}
