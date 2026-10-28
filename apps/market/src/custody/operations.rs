use super::{Custody, keys};
use crate::http::error::{ApiError, Result};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{ContextRecord, Contribution, Opening, Request, Response};
use private_check::{
    BooleanChallengeBinding, KeyOwnerContext, OwnerId, PreparedOwnerMask, TransferBinding,
    TransferChallenge,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tfhe_math::codec::CIPHERTEXT_LIMIT;
use uuid::Uuid;

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

pub struct Work {
    pub account: Uuid,
    pub message: Uuid,
    pub request: Request,
    pub context: ContextRecord,
    pub material: check_workflow::messages::KeyMaterial,
    pub secret: Vec<u8>,
    pub prior: Option<(Uuid, Vec<u8>)>,
}

pub struct Output {
    pub response: ArtifactRef,
    pub private: Option<Vec<u8>>,
}

pub fn execute(custody: &Custody, work: Work) -> Result<Output> {
    let expected = work
        .context
        .restore()
        .map_err(|_| ApiError::Conflict("Invalid private-check context"))?;
    let context = expected.id();
    let store = &custody.artifacts;
    let mut private = None;
    let response: Value = match work.request {
        Request::Number {
            challenge,
            reference,
            ..
        } => {
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
                    .read(&challenge, CIPHERTEXT_LIMIT)
                    .map_err(|_| ApiError::Unavailable)?,
                challenge.digest,
                &expected,
                &owner.evaluate,
            )
            .map_err(|_| ApiError::Conflict("Invalid transfer challenge"))?;
            if challenge.id() != reference {
                return Err(ApiError::Conflict("The challenge changed"));
            }
            let target = target_public(custody, &work.context)?;
            let answer = KeyOwnerContext::new(owner.decrypt, owner.evaluate)
                .map_err(|_| ApiError::Unavailable)?
                .reencrypt_masked(&challenge, &target, &expected)
                .map_err(|_| ApiError::Conflict("Private transfer failed"))?;
            json!(Response::Number {
                context,
                challenge: reference,
                ciphertext: store
                    .write(answer.encoded().as_bytes())
                    .map_err(|_| ApiError::Unavailable)?
            })
        }
        Request::Masks { base, .. } => {
            let target = target_public(custody, &work.context)?;
            let mask = PreparedOwnerMask::generate(
                OwnerId::new(*work.account.as_bytes()).map_err(|_| ApiError::Forbidden)?,
                &expected,
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
            private = Some(custody.vault.seal(
                work.message,
                "response-state",
                &serde_json::to_vec(&saved).map_err(|_| ApiError::Internal)?,
            )?);
            json!(contribution)
        }
        Request::AnswerCommitment {
            base,
            challenge,
            reference,
            ..
        } => {
            if work.account != work.context.manifest.owners[1] {
                return Err(ApiError::Forbidden);
            }
            let owner = keys::load(custody, work.account, &work.material, &work.secret)?;
            let owner = KeyOwnerContext::new(owner.decrypt, owner.evaluate)
                .map_err(|_| ApiError::Unavailable)?;
            let encrypted = owner
                .import_boolean_challenge(
                    BooleanChallengeBinding { context, base },
                    &store
                        .read(&challenge, CIPHERTEXT_LIMIT)
                        .map_err(|_| ApiError::Unavailable)?,
                    challenge.digest,
                    &expected,
                    base,
                )
                .map_err(|_| ApiError::Conflict("Invalid comparison challenge"))?;
            if encrypted.id() != reference {
                return Err(ApiError::Conflict("The challenge changed"));
            }
            let answer = owner
                .prepare_boolean_answer(&encrypted, &expected, base)
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
            private = Some(custody.vault.seal(
                work.message,
                "response-state",
                &serde_json::to_vec(&saved).map_err(|_| ApiError::Internal)?,
            )?);
            json!(Response::AnswerCommitment {
                context,
                challenge: reference,
                commitment
            })
        }
        Request::AnswerReveal {
            context: request_context,
            challenge,
            commitment,
        } => {
            let saved: StoredAnswer = restore(custody, &work)?;
            if saved.context != context
                || request_context != context
                || saved.challenge != challenge
                || saved.commitment != commitment
            {
                return Err(ApiError::Conflict(
                    "Answer does not match the committed response",
                ));
            }
            json!(Response::AnswerReveal {
                context,
                challenge,
                value: saved.value
            })
        }
        Request::Openings {
            context: request_context,
            base,
        } => {
            let saved: StoredMask = restore(custody, &work)?;
            if saved.context != context
                || request_context != context
                || saved.base != base
                || saved.opening.owner != work.account
            {
                return Err(ApiError::Conflict("Mask does not belong to this request"));
            }
            json!(saved.opening)
        }
    };
    Ok(Output {
        response: store
            .write_json(&response)
            .map_err(|_| ApiError::Unavailable)?,
        private,
    })
}

fn restore<T: serde::de::DeserializeOwned>(custody: &Custody, work: &Work) -> Result<T> {
    let (id, sealed) = work.prior.as_ref().ok_or(ApiError::Conflict(
        "The previous response checkpoint is missing",
    ))?;
    let bytes = custody.vault.open(*id, "response-state", sealed)?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::Internal)
}

fn target_public(
    custody: &Custody,
    context: &ContextRecord,
) -> Result<tfhe_math::EncryptionContext> {
    let material = &context.manifest.keys[1];
    let binding = material.binding().map_err(|_| ApiError::Unavailable)?;
    let bytes = custody
        .artifacts
        .read(&material.public_key, tfhe_math::codec::PUBLIC_KEY_LIMIT)
        .map_err(|_| ApiError::Unavailable)?;
    let key = tfhe_math::codec::import_public_key(
        &bytes,
        binding,
        tfhe_math::codec::EncodingLimits::default(),
    )
    .map_err(|_| ApiError::Unavailable)?;
    Ok(tfhe_math::EncryptionContext::new(key))
}
