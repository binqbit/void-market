use super::commitments;
use crate::comparison::answer::{AnswerCommitment, MaskedBooleanAnswer};
use crate::comparison::audit::AuditMaterial;
use crate::comparison::evaluate::{DecodedBoolean, PreparedComparison};
use crate::comparison::owner_masks::OwnerMaskOpening;
use crate::{CheckContext, CheckError, CheckSession, Result};
use zeroize::Zeroizing;

pub(crate) fn decode(
    prepared: &PreparedComparison,
    commitment: AnswerCommitment,
    answer: &MaskedBooleanAnswer,
    openings: [&OwnerMaskOpening; 2],
    expected: &CheckContext,
) -> Result<bool> {
    expected.check_id(prepared.challenge.binding.context)?;
    expected.check_id(answer.context)?;
    expected.check_id(commitment.context)?;
    if answer.challenge != prepared.challenge.id || commitment.challenge != prepared.challenge.id {
        return Err(CheckError::ReferenceMismatch);
    }
    if *answer.value > expected.width().maximum() {
        return Err(CheckError::AnswerOutOfRange);
    }
    if commitments::answer(expected.id, expected.width(), *answer.value)? != commitment.digest {
        return Err(CheckError::CommitmentMismatch);
    }

    if openings[0].binding.owner == openings[1].binding.owner {
        return Err(CheckError::InvalidOwner);
    }
    let mut inner = Zeroizing::new(*answer.value);
    for contribution in prepared.owners {
        let opening = openings
            .iter()
            .find(|opening| opening.binding.owner == contribution.owner)
            .ok_or(CheckError::InvalidOwner)?;
        if opening.binding != contribution {
            return Err(CheckError::CommitmentMismatch);
        }
        if *opening.pad > expected.width().maximum() {
            return Err(CheckError::InvalidMask);
        }
        let actual = commitments::owner(
            expected.id,
            expected.width(),
            contribution.owner,
            *opening.pad,
            &opening.blinding,
        )?;
        if actual != contribution.commitment {
            return Err(CheckError::CommitmentMismatch);
        }
        *inner ^= *opening.pad;
    }

    prepared.masks.masks.decode(expected.width(), *inner)
}

impl CheckSession {
    /// Exact decoding and commitment checks, distinct from proof verification.
    /// Retains Boolean material for the host's later separately authorized audit.
    pub fn decode_boolean(
        &self,
        prepared: PreparedComparison,
        commitment: AnswerCommitment,
        answer: MaskedBooleanAnswer,
        first: OwnerMaskOpening,
        second: OwnerMaskOpening,
    ) -> Result<DecodedBoolean> {
        let value = decode(
            &prepared,
            commitment,
            &answer,
            [&first, &second],
            &self.context,
        )?;

        Ok(DecodedBoolean {
            value,
            context: self.context.id,
            challenge: prepared.challenge.id,
            audit: AuditMaterial {
                prepared,
                commitment,
                answer,
                openings: [first, second],
            },
        })
    }
}
