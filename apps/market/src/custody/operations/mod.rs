mod boolean;
mod number;
mod state;

use super::Custody;
use crate::http::error::{ApiError, Result};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{ContextRecord, Request};
use serde::Serialize;
use uuid::Uuid;

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

/// Dispatch an approved operation. Each role owns its crypto and private checkpoint details.
pub fn execute(custody: &Custody, work: Work) -> Result<Output> {
    let expected = work
        .context
        .restore()
        .map_err(|_| ApiError::Conflict("Invalid private-check context"))?;
    let context = expected.id();

    match &work.request {
        Request::Number {
            challenge,
            reference,
            ..
        } => number::reencrypt(custody, &work, &expected, challenge, *reference),
        Request::Preparation { base, .. } | Request::Masks { base, .. } => {
            boolean::prepare_mask(custody, &work, &expected, *base)
        }
        Request::AnswerCommitment {
            base,
            challenge,
            reference,
            ..
        } => boolean::commit_answer(custody, &work, &expected, *base, challenge, *reference),
        Request::AnswerReveal {
            context: request_context,
            challenge,
            commitment,
        } => boolean::reveal_answer(
            custody,
            &work,
            context,
            *request_context,
            *challenge,
            *commitment,
        ),
        Request::Openings {
            context: request_context,
            base,
        } => boolean::open_mask(custody, &work, context, *request_context, *base),
    }
}

fn respond<T: Serialize>(
    custody: &Custody,
    response: &T,
    private: Option<Vec<u8>>,
) -> Result<Output> {
    // Artifact digests bind bytes. Preserve the existing JSON Value field ordering.
    let response = serde_json::to_value(response).map_err(|_| ApiError::Internal)?;

    Ok(Output {
        response: custody
            .artifacts
            .write_json(&response)
            .map_err(|_| ApiError::Unavailable)?,
        private,
    })
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
