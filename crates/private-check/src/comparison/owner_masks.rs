use crate::checks::commitments;
use crate::values::CiphertextArtifact;
use crate::{CheckContext, CheckError, Digest, OwnerId, Result, random};
use tfhe_math::{
    DataType, EncryptionContext, EvaluationContext, PublicValue, codec::EncodedObject,
};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerContributionBinding {
    pub context: Digest,
    pub base: Digest,
    pub owner: OwnerId,
    pub commitment: Digest,
}

/// Deliver the encrypted pad only to its permitted computation/verifier scope.
pub struct OwnerMaskContribution {
    pub(crate) binding: OwnerContributionBinding,
    pub(crate) artifact: CiphertextArtifact,
}

impl OwnerMaskContribution {
    pub fn binding(&self) -> OwnerContributionBinding {
        self.binding
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.artifact.wire
    }

    /// Structural import does not prove the commitment/encryption knowledge bridge.
    pub fn import(
        binding: OwnerContributionBinding,
        bytes: &[u8],
        digest: Digest,
        expected: &CheckContext,
        expected_base: Digest,
        target: &EvaluationContext,
    ) -> Result<Self> {
        expected.check_id(binding.context)?;
        expected.check_owner(binding.owner)?;
        if binding.base != expected_base {
            return Err(CheckError::CommitmentMismatch);
        }
        if target.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let artifact =
            CiphertextArtifact::import(bytes, digest, DataType::Uint(expected.width()), target)?;
        Ok(Self { binding, artifact })
    }
}

pub struct PreparedOwnerMask {
    contribution: OwnerMaskContribution,
    pad: Zeroizing<u64>,
    blinding: Zeroizing<[u8; 32]>,
}

impl PreparedOwnerMask {
    pub fn generate(
        owner: OwnerId,
        expected: &CheckContext,
        base: Digest,
        target: &EncryptionContext,
    ) -> Result<Self> {
        expected.check_owner(owner)?;
        if target.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }
        let pad = random::scalar(expected.width())?;
        let blinding = random::bytes::<32>()?;

        let commitment = commitments::owner(expected.id, expected.width(), owner, *pad, &blinding)?;
        let artifact =
            CiphertextArtifact::encrypt(target, PublicValue::uint(expected.width(), *pad)?)?;
        let binding = OwnerContributionBinding {
            context: expected.id,
            base,
            owner,
            commitment,
        };

        Ok(Self {
            contribution: OwnerMaskContribution { binding, artifact },
            pad,
            blinding,
        })
    }

    pub fn contribution(&self) -> &OwnerMaskContribution {
        &self.contribution
    }

    /// Explicit local opening primitive; the host must authorize its timing/audience.
    pub fn open(self) -> OwnerMaskOpening {
        OwnerMaskOpening {
            binding: self.contribution.binding,
            pad: self.pad,
            blinding: self.blinding,
        }
    }
}

pub struct OwnerMaskOpening {
    pub(crate) binding: OwnerContributionBinding,
    pub(crate) pad: Zeroizing<u64>,
    pub(crate) blinding: Zeroizing<[u8; 32]>,
}

impl OwnerMaskOpening {
    pub fn binding(&self) -> OwnerContributionBinding {
        self.binding
    }

    /// Explicit disclosure for an already authorized protected opening channel.
    pub fn expose(&self) -> (u64, Zeroizing<[u8; 32]>) {
        (*self.pad, Zeroizing::new(*self.blinding))
    }

    pub fn from_parts(
        binding: OwnerContributionBinding,
        pad: u64,
        blinding: [u8; 32],
        expected: &CheckContext,
    ) -> Result<Self> {
        let opening = Self {
            binding,
            pad: Zeroizing::new(pad),
            blinding: Zeroizing::new(blinding),
        };
        expected.check_id(binding.context)?;
        expected.check_owner(binding.owner)?;
        if pad > expected.width().maximum() {
            return Err(CheckError::InvalidMask);
        }
        if commitments::owner(
            expected.id,
            expected.width(),
            binding.owner,
            pad,
            &opening.blinding,
        )? != binding.commitment
        {
            return Err(CheckError::CommitmentMismatch);
        }

        Ok(opening)
    }
}
