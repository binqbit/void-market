use super::masks::PreparedBooleanMasks;
use super::owner_masks::{OwnerContributionBinding, OwnerMaskContribution};
use crate::checks::commitments;
use crate::values::CiphertextArtifact;
use crate::{
    CheckContext, CheckError, CheckSession, Comparison, Digest, NumberInput, Result,
    TransferredNumber,
};
use tfhe_math::{DataType, EvaluationContext, PublicValue, codec::EncodedObject};

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
        let bit = match self.context.comparison {
            Comparison::Equal => math.eq(x, v)?,
            Comparison::NotEqual => math.ne(x, v)?,
            Comparison::Less => math.lt(x, v)?,
            Comparison::LessOrEqual => math.le(x, v)?,
            Comparison::Greater => math.gt(x, v)?,
            Comparison::GreaterOrEqual => math.ge(x, v)?,
        };

        let width = self.context.width();
        let numeric_bit = math.bool_to_uint(&bit, width)?;
        let product = math.mul_in_width(width, &numeric_bit, &masks.encrypted[0].value)?;
        let sum = math.add_in_width(width, &product.value, &masks.encrypted[1].value)?;
        let base = math.xor(&sum.value, &masks.encrypted[2].value)?;
        let first_pad = math.xor(&base, &owners[0].artifact.value)?;
        let protected = math.xor(&first_pad, &owners[1].artifact.value)?;

        let limit = PublicValue::uint(width, width.maximum() / 2)?.into();
        let number_checks = [
            CiphertextArtifact::from_value(math.le(x, &limit)?)?,
            CiphertextArtifact::from_value(math.le(v, &limit)?)?,
        ];
        let artifact = CiphertextArtifact::from_value(protected)?;
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
            arithmetic_checks: [
                CiphertextArtifact::from_value(product.flag)?,
                CiphertextArtifact::from_value(sum.flag)?,
            ],
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
