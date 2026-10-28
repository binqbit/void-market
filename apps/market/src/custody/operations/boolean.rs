use super::{Custody, Output, Work, respond, state, target_public};
use crate::{
    custody::keys,
    http::error::{ApiError, Result},
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{Contribution, Opening, Response};
use private_check::{
    BooleanChallengeBinding, CheckContext, Digest, KeyOwnerContext, OwnerId, PreparedOwnerMask,
};
use serde::{Deserialize, Serialize};
use tfhe_math::codec::CIPHERTEXT_LIMIT;

#[derive(Serialize, Deserialize)]
struct StoredMask {
    context: [u8; 32],
    base: [u8; 32],
    opening: Opening,
}

#[derive(Serialize, Deserialize)]
struct StoredAnswer {
    context: [u8; 32],
    challenge: [u8; 32],
    commitment: [u8; 32],
    base: [u8; 32],
    value: u64,
}

pub(super) fn prepare_mask(
    custody: &Custody,
    work: &Work,
    expected: &CheckContext,
    base: Digest,
) -> Result<Output> {
    let context = expected.id();
    let store = &custody.artifacts;
    let target = target_public(custody, &work.context)?;
    let mask = PreparedOwnerMask::generate(
        OwnerId::new(*work.account.as_bytes()).map_err(|_| ApiError::Forbidden)?,
        expected,
        base,
        &target,
    )
    .map_err(|_| ApiError::Conflict("Mask preparation failed"))?;

    let bound = mask.contribution().binding();
    let contribution = Contribution {
        owner: work.account,
        commitment: bound.commitment,
        ciphertext: store
            .write(mask.contribution().encoded().as_bytes())
            .map_err(|_| ApiError::Unavailable)?,
    };

    let opening = mask.open();
    let (value, blinding) = opening.expose();
    let saved = StoredMask {
        context,
        base,
        opening: Opening {
            owner: work.account,
            commitment: bound.commitment,
            value,
            blinding: *blinding,
        },
    };
    let private = state::seal(custody, work, &saved)?;
    respond(custody, &contribution, Some(private))
}

pub(super) fn commit_answer(
    custody: &Custody,
    work: &Work,
    expected: &CheckContext,
    base: Digest,
    challenge: &ArtifactRef,
    reference: Digest,
) -> Result<Output> {
    let context = expected.id();
    let store = &custody.artifacts;
    if work.account != work.context.manifest.owners[1] {
        return Err(ApiError::Forbidden);
    }

    let owner = keys::load(custody, work.account, &work.material, &work.secret)?;
    let owner =
        KeyOwnerContext::new(owner.decrypt, owner.evaluate).map_err(|_| ApiError::Unavailable)?;
    let encrypted = owner
        .import_boolean_challenge(
            BooleanChallengeBinding { context, base },
            &store
                .read(challenge, CIPHERTEXT_LIMIT)
                .map_err(|_| ApiError::Unavailable)?,
            challenge.digest,
            expected,
            base,
        )
        .map_err(|_| ApiError::Conflict("Invalid comparison challenge"))?;
    if encrypted.id() != reference {
        return Err(ApiError::Conflict("The challenge changed"));
    }

    let answer = owner
        .prepare_boolean_answer(&encrypted, expected, base)
        .map_err(|_| ApiError::Conflict("Invalid protected answer"))?;
    let commitment = answer.commitment().digest;
    let revealed = answer
        .reveal(base)
        .map_err(|_| ApiError::Conflict("Invalid protected answer"))?;

    let saved = StoredAnswer {
        context,
        challenge: reference,
        commitment,
        base,
        value: revealed.value_for_protocol(),
    };
    let private = state::seal(custody, work, &saved)?;
    respond(
        custody,
        &Response::AnswerCommitment {
            context,
            challenge: reference,
            commitment,
        },
        Some(private),
    )
}

pub(super) fn reveal_answer(
    custody: &Custody,
    work: &Work,
    context: Digest,
    request_context: Digest,
    challenge: Digest,
    commitment: Digest,
) -> Result<Output> {
    let saved: StoredAnswer = state::restore(custody, work)?;
    if saved.context != context
        || request_context != context
        || saved.challenge != challenge
        || saved.commitment != commitment
    {
        return Err(ApiError::Conflict(
            "Answer does not match the committed response",
        ));
    }
    respond(
        custody,
        &Response::AnswerReveal {
            context,
            challenge,
            value: saved.value,
        },
        None,
    )
}

pub(super) fn open_mask(
    custody: &Custody,
    work: &Work,
    context: Digest,
    request_context: Digest,
    base: Digest,
) -> Result<Output> {
    let saved: StoredMask = state::restore(custody, work)?;
    if saved.context != context
        || request_context != context
        || saved.base != base
        || saved.opening.owner != work.account
    {
        return Err(ApiError::Conflict("Mask does not belong to this request"));
    }
    respond(custody, &saved.opening, None)
}
