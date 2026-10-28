use super::{ComparisonEngine, Progress, Step};
use crate::error::{Error, Result};
use check_workflow::messages::{Contribution, NumberChallenge, NumberResponse, Request};
use private_check::{
    Digest, NumberInput, OwnerMaskContribution, PreparedBooleanMasks, PreparedTransfer,
    TransferBinding, TransferResponse, TransferredNumber,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tfhe_math::codec::CIPHERTEXT_LIMIT;

impl ComparisonEngine {
    pub(super) fn prepare(&mut self, source: &NumberInput) -> Result<Progress> {
        let masks = self.session.prepare_boolean_masks()?;
        let transfer = self.session.prepare_number_transfer(source)?;
        let challenge = self
            .artifacts
            .write(transfer.challenge().encoded().as_bytes())?;
        let request = Request::Preparation {
            context: self.context.clone(),
            base: masks.commitment(),
            number: NumberChallenge {
                challenge,
                reference: transfer.challenge().id(),
            },
        };
        self.step = Some(Step::Preparation { transfer, masks });
        self.wait(request)
    }

    pub(super) fn finish_preparation(
        &mut self,
        transfer: PreparedTransfer,
        masks: PreparedBooleanMasks,
        contributions: [Contribution; 2],
        number: NumberResponse,
        demo_close: Option<i64>,
    ) -> Result<Progress> {
        self.ensure_release_allowed(demo_close)?;

        let context = self.session.context();
        let contributions =
            self.import_contributions(context.id(), masks.commitment(), &contributions)?;
        let response = TransferResponse::import(
            TransferBinding {
                context: context.id(),
                reference: number.challenge,
            },
            &self.artifacts.read(&number.ciphertext, CIPHERTEXT_LIMIT)?,
            number.ciphertext.digest,
            transfer.challenge(),
            context,
            &self.target_evaluation,
        )?;
        let number = self.session.finish_number_transfer(transfer, response)?;

        self.compare(number, masks, contributions)
    }

    fn ensure_release_allowed(&self, demo_close: Option<i64>) -> Result<()> {
        let Some(release) = self.context.manifest.release_at_ms else {
            return Ok(());
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Response)?
            .as_millis();
        let released_early = demo_close
            .is_some_and(|closed| closed >= 0 && closed <= release && now >= closed as u128);
        if release < 0 || (now < release as u128 && !released_early) {
            return Err(Error::Response);
        }
        Ok(())
    }

    fn import_contributions(
        &self,
        context: Digest,
        base: Digest,
        contributions: &[Contribution; 2],
    ) -> Result<[OwnerMaskContribution; 2]> {
        let import = |value: &Contribution| -> Result<OwnerMaskContribution> {
            Ok(OwnerMaskContribution::import(
                value.binding(context, base)?,
                &self.artifacts.read(&value.ciphertext, CIPHERTEXT_LIMIT)?,
                value.ciphertext.digest,
                self.session.context(),
                base,
                &self.target_evaluation,
            )?)
        };
        Ok([import(&contributions[0])?, import(&contributions[1])?])
    }

    fn compare(
        &mut self,
        number: TransferredNumber,
        masks: PreparedBooleanMasks,
        contributions: [OwnerMaskContribution; 2],
    ) -> Result<Progress> {
        let base = masks.commitment();
        let comparison = self.session.compare_protected(
            &number,
            &self.target,
            masks,
            &contributions[0],
            &contributions[1],
        )?;
        let challenge = self
            .artifacts
            .write(comparison.challenge().encoded().as_bytes())?;
        let request = Request::AnswerCommitment {
            context: self.context.clone(),
            base,
            challenge,
            reference: comparison.challenge().id(),
        };
        self.step = Some(Step::Commitment(comparison));
        self.wait(request)
    }
}
