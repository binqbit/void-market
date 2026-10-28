use crate::{
    http::error::{ApiError, Result},
    messages::model::ActionKind,
};
use check_workflow::messages::{ContextRecord, Request};
use serde_json::json;
use uuid::Uuid;

/// Validate the stage transition before it can create any participant obligation.
pub(super) fn describe(
    request: &Request,
    previous: Option<&Request>,
    saved: Option<&ContextRecord>,
    members: &[Uuid],
) -> Result<(ContextRecord, Vec<(Uuid, ActionKind)>)> {
    let context = match request {
        Request::Preparation { context, .. }
        | Request::Number { context, .. }
        | Request::Masks { context, .. }
        | Request::AnswerCommitment { context, .. } => context.clone(),
        _ => saved
            .cloned()
            .ok_or(ApiError::Conflict("Missing comparison context"))?,
    };
    if members != context.manifest.owners {
        return Err(ApiError::Forbidden);
    }
    if saved.is_some_and(|saved| json!(saved) != json!(context)) {
        return Err(ApiError::Conflict("Comparison context changed"));
    }
    let id = context
        .restore()
        .map_err(|_| ApiError::Conflict("Invalid comparison context"))?
        .id();
    let both = |kind| {
        members
            .iter()
            .copied()
            .map(|account| (account, kind))
            .collect()
    };
    let actions = match (request, previous) {
        (Request::Preparation { .. }, None) => both(ActionKind::MaskContribution),
        (Request::AnswerCommitment { .. }, Some(Request::Preparation { .. })) => {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::Number { .. }, None) => vec![(members[0], ActionKind::NumberTransfer)],
        (Request::Masks { context, .. }, None) if context.manifest.release_at_ms.is_some() => {
            both(ActionKind::MaskContribution)
        }
        (Request::Number { context, .. }, Some(Request::Masks { .. }))
            if context.manifest.release_at_ms.is_some() =>
        {
            vec![(members[0], ActionKind::NumberTransfer)]
        }
        (Request::Masks { .. }, Some(Request::Number { .. })) => both(ActionKind::MaskContribution),
        (Request::AnswerCommitment { .. }, Some(Request::Masks { .. })) => {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::AnswerCommitment { context, .. }, Some(Request::Number { .. }))
            if context.manifest.release_at_ms.is_some() =>
        {
            vec![(members[1], ActionKind::AnswerCommitment)]
        }
        (Request::AnswerReveal { context, .. }, Some(Request::AnswerCommitment { .. }))
            if *context == id =>
        {
            vec![(members[1], ActionKind::AnswerReveal)]
        }
        (Request::Openings { context, .. }, Some(Request::AnswerReveal { .. }))
            if *context == id =>
        {
            both(ActionKind::MaskOpening)
        }
        _ => return Err(ApiError::Conflict("Unexpected private-check stage")),
    };
    Ok((context, actions))
}
