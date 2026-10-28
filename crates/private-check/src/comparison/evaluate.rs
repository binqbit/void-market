use super::owner_masks::{OwnerContributionBinding, OwnerMaskContribution};
use super::{encoding, masks::PreparedBooleanMasks};
use crate::checks::{commitments, number::encrypted_range_check};
use crate::values::CiphertextArtifact;
use crate::{
    CheckContext, CheckError, CheckSession, Comparison, Digest, NumberInput, Result,
    TransferredNumber,
};
use tfhe_math::{DataType, EvaluationContext, MathSession, TfheValue, codec::EncodedObject};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanChallengeBinding {
    pub context: Digest,
    pub base: Digest,
}

pub struct BooleanChallenge {
    pub(crate) binding: BooleanChallengeBinding,
    pub(crate) artifact: CiphertextArtifact,
    pub(crate) id: Digest,
}

impl BooleanChallenge {
    pub fn binding(&self) -> BooleanChallengeBinding {
        self.binding
    }

    pub fn id(&self) -> Digest {
        self.id
    }

    pub fn encoded(&self) -> &EncodedObject {
        &self.artifact.wire
    }

    pub fn import(
        binding: BooleanChallengeBinding,
        bytes: &[u8],
        digest: Digest,
        expected: &CheckContext,
        expected_base: Digest,
        target: &EvaluationContext,
    ) -> Result<Self> {
        expected.check_id(binding.context)?;
        if binding.base != expected_base {
            return Err(CheckError::CommitmentMismatch);
        }
        if target.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let artifact =
            CiphertextArtifact::import(bytes, digest, DataType::Uint(expected.width()), target)?;
        let id =
            commitments::message_reference(binding.context, binding.base, artifact.wire.digest())?;
        Ok(Self {
            binding,
            artifact,
            id,
        })
    }
}

pub struct PreparedComparison {
    pub(crate) challenge: BooleanChallenge,
    pub(crate) masks: PreparedBooleanMasks,
    pub(crate) owners: [OwnerContributionBinding; 2],
    pub(crate) arithmetic_checks: [CiphertextArtifact; 2],
    pub(crate) number_checks: [CiphertextArtifact; 2],
}

impl PreparedComparison {
    pub fn challenge(&self) -> &BooleanChallenge {
        &self.challenge
    }

    pub fn base_commitment(&self) -> Digest {
        self.masks.commitment
    }
    /// Multiplication and addition overflow flags, kept encrypted in the target domain.
    pub fn arithmetic_checks(&self) -> [&EncodedObject; 2] {
        [
            &self.arithmetic_checks[0].wire,
            &self.arithmetic_checks[1].wire,
        ]
    }
    /// Fixed LE(L) checks for transferred x and original v; no plaintext verdict.
    pub fn number_checks(&self) -> [&EncodedObject; 2] {
        [&self.number_checks[0].wire, &self.number_checks[1].wire]
    }
}

/// An exact mathematical decode, not a cryptographic verification receipt.
pub struct DecodedBoolean {
    pub(crate) value: bool,
    pub(crate) context: Digest,
    pub(crate) challenge: Digest,
    pub(crate) audit: super::audit::AuditMaterial,
}

impl DecodedBoolean {
    pub fn value(&self) -> bool {
        self.value
    }

    pub fn context(&self) -> Digest {
        self.context
    }

    pub fn challenge(&self) -> Digest {
        self.challenge
    }
}

impl CheckSession {
    pub fn compare_protected(
        &self,
        transferred: &TransferredNumber,
        target: &NumberInput,
        masks: PreparedBooleanMasks,
        first: &OwnerMaskContribution,
        second: &OwnerMaskContribution,
    ) -> Result<PreparedComparison> {
        self.context.check_id(transferred.context)?;
        self.context.check_id(masks.context)?;
        if transferred.source != self.context.source || target.binding != self.context.target {
            return Err(CheckError::ReferenceMismatch);
        }
        let owners = self.order_contributions(first, second, masks.commitment)?;
        masks.masks.endpoints(self.context.width())?;

        let math = self.target.math();
        let (x, v) = (&transferred.value, &target.value);
        let width = self.context.width();
        let bit = compare_values(&math, self.context.comparison, x, v)?;
        let protected = encoding::protect(&math, width, &bit, &masks, owners)?;
        let number_checks = [
            encrypted_range_check(&math, width, x)?,
            encrypted_range_check(&math, width, v)?,
        ];

        let artifact = protected.artifact;
        let binding = BooleanChallengeBinding {
            context: self.context.id,
            base: masks.commitment,
        };
        let id =
            commitments::message_reference(binding.context, binding.base, artifact.wire.digest())?;

        Ok(PreparedComparison {
            challenge: BooleanChallenge {
                binding,
                artifact,
                id,
            },
            owners: [owners[0].binding, owners[1].binding],
            masks,
            arithmetic_checks: protected.arithmetic_checks,
            number_checks,
        })
    }

    fn order_contributions<'a>(
        &self,
        first: &'a OwnerMaskContribution,
        second: &'a OwnerMaskContribution,
        base: Digest,
    ) -> Result<[&'a OwnerMaskContribution; 2]> {
        for contribution in [first, second] {
            self.context.check_id(contribution.binding.context)?;
            self.context.check_owner(contribution.binding.owner)?;
            if contribution.binding.base != base {
                return Err(CheckError::CommitmentMismatch);
            }
        }
        if first.binding.owner == second.binding.owner {
            return Err(CheckError::InvalidOwner);
        }

        if first.binding.owner == self.context.owners[0] {
            Ok([first, second])
        } else {
            Ok([second, first])
        }
    }
}

fn compare_values(
    math: &MathSession,
    comparison: Comparison,
    left: &TfheValue,
    right: &TfheValue,
) -> Result<TfheValue> {
    Ok(match comparison {
        Comparison::Equal => math.eq(left, right)?,
        Comparison::NotEqual => math.ne(left, right)?,
        Comparison::Less => math.lt(left, right)?,
        Comparison::LessOrEqual => math.le(left, right)?,
        Comparison::Greater => math.gt(left, right)?,
        Comparison::GreaterOrEqual => math.ge(left, right)?,
    })
}
