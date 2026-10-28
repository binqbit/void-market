use super::{ComparisonEngine, Progress, Step};
use crate::error::Result;
use check_workflow::messages::{Opening, Request};
use private_check::{
    AnswerCommitment, Digest, MaskedBooleanAnswer, OwnerContributionBinding, OwnerId,
    OwnerMaskOpening, PreparedComparison,
};

impl ComparisonEngine {
    pub(super) fn request_answer_reveal(
        &mut self,
        comparison: PreparedComparison,
        digest: Digest,
    ) -> Result<Progress> {
        let context = self.session.context().id();
        let challenge = comparison.challenge().id();
        let request = Request::AnswerReveal {
            context,
            challenge,
            commitment: digest,
        };
        self.step = Some(Step::Answer {
            comparison,
            commitment: AnswerCommitment {
                context,
                challenge,
                digest,
            },
        });
        self.wait(request)
    }

    pub(super) fn request_owner_openings(
        &mut self,
        comparison: PreparedComparison,
        commitment: AnswerCommitment,
        value: u64,
    ) -> Result<Progress> {
        let expected = self.session.context();
        let context = expected.id();
        let answer =
            MaskedBooleanAnswer::from_parts(context, comparison.challenge().id(), value, expected)?;
        let request = Request::Openings {
            context,
            base: comparison.base_commitment(),
        };
        self.step = Some(Step::Openings {
            comparison,
            commitment,
            answer,
        });
        self.wait(request)
    }

    pub(super) fn complete_comparison(
        &self,
        comparison: PreparedComparison,
        commitment: AnswerCommitment,
        answer: MaskedBooleanAnswer,
        openings: [Opening; 2],
    ) -> Result<Progress> {
        let base = comparison.base_commitment();
        let first = self.import_opening(&openings[0], base)?;
        let second = self.import_opening(&openings[1], base)?;
        let decoded = self
            .session
            .decode_boolean(comparison, commitment, answer, first, second)?;

        let result = self.artifacts.write_json(&serde_json::json!({
            "schema_version": 1,
            "context": self.session.context().id(),
            "value": decoded.value(),
            "assurance": "honest_role_mechanics",
            "proof_status": "unavailable",
            "final_audit": "not_released",
        }))?;
        Ok(Progress::Complete(result))
    }

    fn import_opening(&self, value: &Opening, base: Digest) -> Result<OwnerMaskOpening> {
        let expected = self.session.context();
        Ok(OwnerMaskOpening::from_parts(
            OwnerContributionBinding {
                context: expected.id(),
                base,
                owner: OwnerId::new(*value.owner.as_bytes())?,
                commitment: value.commitment,
            },
            value.value,
            value.blinding,
            expected,
        )?)
    }
}
