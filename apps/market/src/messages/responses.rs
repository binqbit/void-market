use crate::http::error::{ApiError, Result};
use check_workflow::messages::{
    ContextRecord, Contribution, NumberResponse, Opening, Request, Response,
};
use serde_json::Value;

/// Assemble a completed request from independently received owner responses.
pub(super) fn assemble(
    request: Request,
    context: &ContextRecord,
    pieces: Vec<(String, Value)>,
) -> Result<Response> {
    let response = match request {
        Request::Preparation {
            base,
            number: expected,
            ..
        } => {
            if pieces.len() != 3 {
                return Err(ApiError::Conflict("Incomplete preparation responses"));
            }
            let contributions: Vec<Contribution> = pieces
                .iter()
                .filter(|(kind, _)| kind == "mask_contribution")
                .map(|(_, value)| serde_json::from_value(value.clone()))
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| ApiError::Internal)?;
            let context_id = context.restore().map_err(|_| ApiError::Internal)?.id();
            let value = pieces
                .iter()
                .find(|(kind, _)| kind == "number_transfer")
                .ok_or(ApiError::Internal)?;
            let number = match serde_json::from_value::<Response>(value.1.clone())
                .map_err(|_| ApiError::Internal)?
            {
                Response::Number {
                    context: id,
                    challenge,
                    ciphertext,
                } if id == context_id && challenge == expected.reference => NumberResponse {
                    challenge,
                    ciphertext,
                },
                _ => return Err(ApiError::Conflict("Invalid number response")),
            };
            Response::Preparation {
                context: context_id,
                base,
                contributions: contributions.try_into().map_err(|_| ApiError::Internal)?,
                number,
            }
        }
        Request::Masks { base, .. } => {
            let contributions: Vec<Contribution> = pieces
                .into_iter()
                .map(|(_, value)| serde_json::from_value(value))
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| ApiError::Internal)?;
            Response::Masks {
                context: context.restore().map_err(|_| ApiError::Internal)?.id(),
                base,
                contributions: contributions.try_into().map_err(|_| ApiError::Internal)?,
            }
        }
        Request::Openings { context, base } => {
            let openings: Vec<Opening> = pieces
                .into_iter()
                .map(|(_, value)| serde_json::from_value(value))
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| ApiError::Internal)?;
            Response::Openings {
                context,
                base,
                openings: openings.try_into().map_err(|_| ApiError::Internal)?,
            }
        }
        _ => {
            if pieces.len() != 1 {
                return Err(ApiError::Conflict("Unexpected response count"));
            }
            serde_json::from_value(pieces.into_iter().next().ok_or(ApiError::Internal)?.1)
                .map_err(|_| ApiError::Internal)?
        }
    };
    Ok(response)
}
