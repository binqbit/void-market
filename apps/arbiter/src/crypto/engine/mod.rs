mod answer;
mod inputs;
mod preparation;

use crate::{
    artifacts::Artifacts,
    error::{Error, Result},
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{CheckManifest, ContextRecord, Request, Response};
use private_check::{
    AnswerCommitment, CheckSession, MaskedBooleanAnswer, NumberInput, PreparedBooleanMasks,
    PreparedComparison, PreparedTransfer,
};
use tfhe_math::EvaluationContext;

pub enum Progress {
    Waiting(ArtifactRef),
    Complete(ArtifactRef),
}

enum Step {
    Preparation {
        transfer: PreparedTransfer,
        masks: PreparedBooleanMasks,
    },
    Commitment(PreparedComparison),
    Answer {
        comparison: PreparedComparison,
        commitment: AnswerCommitment,
    },
    Openings {
        comparison: PreparedComparison,
        commitment: AnswerCommitment,
        answer: MaskedBooleanAnswer,
    },
}

/// One in-memory attempt. Numeric masks cannot be serialized or reconstructed.
/// A process restart invalidates this attempt; the durable job records interruption.
pub struct ComparisonEngine {
    artifacts: Artifacts,
    context: ContextRecord,
    session: CheckSession,
    target: NumberInput,
    target_evaluation: EvaluationContext,
    step: Option<Step>,
}

impl ComparisonEngine {
    pub fn start(artifacts: Artifacts, manifest: CheckManifest) -> Result<(Self, Progress)> {
        let inputs = inputs::load(&artifacts, manifest)?;

        let mut engine = Self {
            artifacts,
            context: inputs.context,
            session: inputs.session,
            target: inputs.target,
            target_evaluation: inputs.target_evaluation,
            step: None,
        };

        let progress = engine.prepare(&inputs.source)?;
        Ok((engine, progress))
    }

    pub fn advance(&mut self, response: Response) -> Result<Progress> {
        self.advance_with_release(response, None)
    }

    /// The caller must obtain this time from the authenticated gateway for this check.
    #[cfg(feature = "demo-mode")]
    pub fn advance_after_demo_close(
        &mut self,
        response: Response,
        closed_at_ms: i64,
    ) -> Result<Progress> {
        self.advance_with_release(response, Some(closed_at_ms))
    }

    fn advance_with_release(
        &mut self,
        response: Response,
        demo_close: Option<i64>,
    ) -> Result<Progress> {
        let step = self.step.take().ok_or(Error::Response)?;
        let expected = self.session.context();
        let context_id = expected.id();

        match (step, response) {
            (
                Step::Preparation { transfer, masks },
                Response::Preparation {
                    context,
                    base,
                    contributions,
                    number,
                },
            ) if context == context_id
                && base == masks.commitment()
                && number.challenge == transfer.challenge().id() =>
            {
                self.finish_preparation(transfer, masks, contributions, number, demo_close)
            }
            (
                Step::Commitment(comparison),
                Response::AnswerCommitment {
                    context,
                    challenge,
                    commitment,
                },
            ) if context == context_id && challenge == comparison.challenge().id() => {
                self.request_answer_reveal(comparison, commitment)
            }
            (
                Step::Answer {
                    comparison,
                    commitment,
                },
                Response::AnswerReveal {
                    context,
                    challenge,
                    value,
                },
            ) if context == context_id && challenge == comparison.challenge().id() => {
                self.request_owner_openings(comparison, commitment, value)
            }
            (
                Step::Openings {
                    comparison,
                    commitment,
                    answer,
                },
                Response::Openings {
                    context,
                    base,
                    openings,
                },
            ) if context == context_id && base == comparison.base_commitment() => {
                self.complete_comparison(comparison, commitment, answer, openings)
            }
            _ => Err(Error::Response),
        }
    }

    fn wait(&self, request: Request) -> Result<Progress> {
        Ok(Progress::Waiting(self.artifacts.write_json(&request)?))
    }
}
