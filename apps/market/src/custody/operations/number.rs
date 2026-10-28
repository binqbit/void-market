use super::{Custody, Output, Work, respond, target_public};
use crate::{
    custody::keys,
    http::error::{ApiError, Result},
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::Response;
use private_check::{CheckContext, Digest, KeyOwnerContext, TransferBinding, TransferChallenge};
use tfhe_math::codec::CIPHERTEXT_LIMIT;

pub(super) fn reencrypt(
    custody: &Custody,
    work: &Work,
    expected: &CheckContext,
    challenge: &ArtifactRef,
    reference: Digest,
) -> Result<Output> {
    let context = expected.id();
    let store = &custody.artifacts;
    if work.account != work.context.manifest.owners[0] {
        return Err(ApiError::Forbidden);
    }

    let owner = keys::load(custody, work.account, &work.material, &work.secret)?;
    let challenge = TransferChallenge::import(
        TransferBinding {
            context,
            reference: expected.source().native_digest(),
        },
        &store
            .read(challenge, CIPHERTEXT_LIMIT)
            .map_err(|_| ApiError::Unavailable)?,
        challenge.digest,
        expected,
        &owner.evaluate,
    )
    .map_err(|_| ApiError::Conflict("Invalid transfer challenge"))?;
    if challenge.id() != reference {
        return Err(ApiError::Conflict("The challenge changed"));
    }

    let target = target_public(custody, &work.context)?;
    let answer = KeyOwnerContext::new(owner.decrypt, owner.evaluate)
        .map_err(|_| ApiError::Unavailable)?
        .reencrypt_masked(&challenge, &target, expected)
        .map_err(|_| ApiError::Conflict("Private transfer failed"))?;

    respond(
        custody,
        &Response::Number {
            context,
            challenge: reference,
            ciphertext: store
                .write(answer.encoded().as_bytes())
                .map_err(|_| ApiError::Unavailable)?,
        },
        None,
    )
}
