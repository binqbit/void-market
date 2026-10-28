use super::evaluate::{BooleanChallenge, BooleanChallengeBinding};
use crate::checks::commitments;
use crate::{CheckContext, CheckError, Digest, KeyOwnerContext, Result};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerCommitment {
    pub context: Digest,
    pub challenge: Digest,
    pub digest: Digest,
}

pub struct PendingBooleanAnswer {
    pub(crate) commitment: AnswerCommitment,
    pub(crate) base: Digest,
    pub(crate) value: Zeroizing<u64>,
}

impl PendingBooleanAnswer {
    pub fn commitment(&self) -> AnswerCommitment {
        self.commitment
    }

    /// The host arranges commit-before-reveal and independently authorizes disclosure.
    pub fn reveal(self, original_base: Digest) -> Result<MaskedBooleanAnswer> {
        if original_base != self.base {
            return Err(CheckError::CommitmentMismatch);
        }

        Ok(MaskedBooleanAnswer {
            context: self.commitment.context,
            challenge: self.commitment.challenge,
            value: self.value,
        })
    }
}

/// Clear outer Boolean response Y; never a numeric transfer response or raw bit.
pub struct MaskedBooleanAnswer {
    pub(crate) context: Digest,
    pub(crate) challenge: Digest,
    pub(crate) value: Zeroizing<u64>,
}

impl MaskedBooleanAnswer {
    pub fn context(&self) -> Digest {
        self.context
    }

    pub fn challenge(&self) -> Digest {
        self.challenge
    }

    /// Explicit exposure for the authorized protected response channel.
    pub fn value_for_protocol(&self) -> u64 {
        *self.value
    }

    pub fn from_parts(
        context: Digest,
        challenge: Digest,
        value: u64,
        expected: &CheckContext,
    ) -> Result<Self> {
        let value = Zeroizing::new(value);
        expected.check_id(context)?;
        if *value > expected.width().maximum() {
            return Err(CheckError::AnswerOutOfRange);
        }

        Ok(Self {
            context,
            challenge,
            value,
        })
    }
}

impl KeyOwnerContext {
    pub fn import_boolean_challenge(
        &self,
        binding: BooleanChallengeBinding,
        bytes: &[u8],
        digest: Digest,
        expected: &CheckContext,
        original_base: Digest,
    ) -> Result<BooleanChallenge> {
        BooleanChallenge::import(
            binding,
            bytes,
            digest,
            expected,
            original_base,
            &self.evaluation,
        )
    }

    /// Caller verifies authorization/evaluation/privacy evidence before invoking Dec.
    pub fn prepare_boolean_answer(
        &self,
        challenge: &BooleanChallenge,
        expected: &CheckContext,
        original_base: Digest,
    ) -> Result<PendingBooleanAnswer> {
        expected.check_id(challenge.binding.context)?;
        if challenge.binding.base != original_base {
            return Err(CheckError::CommitmentMismatch);
        }
        if self.binding() != expected.target.key {
            return Err(CheckError::KeyDomainMismatch);
        }

        let value = Zeroizing::new(
            self.decryption
                .decrypt(&challenge.artifact.value)?
                .as_uint()?,
        );
        if *value > expected.width().maximum() {
            return Err(CheckError::AnswerOutOfRange);
        }
        let digest = commitments::answer(expected.id, expected.width(), *value)?;

        Ok(PendingBooleanAnswer {
            commitment: AnswerCommitment {
                context: expected.id,
                challenge: challenge.id,
                digest,
            },
            base: original_base,
            value,
        })
    }
}
