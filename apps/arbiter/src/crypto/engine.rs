use crate::{
    artifacts::Artifacts,
    error::{Error, Result},
};
use arbiter_protocol::ArtifactRef;
use check_workflow::messages::{
    self as records, Assurance, CheckManifest, ContextRecord, Contribution, Request, Response,
};
use private_check::{
    AnswerCommitment, CheckContext, CheckSession, ComputationContext, MaskedBooleanAnswer,
    NumberInput, OwnerId, OwnerMaskContribution, OwnerMaskOpening, PreparedBooleanMasks,
    PreparedComparison, PreparedTransfer, TransferBinding, TransferResponse, TransferredNumber,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tfhe_math::{
    EncryptionContext, EvaluationContext,
    codec::{
        CIPHERTEXT_LIMIT, EVALUATION_KEY_LIMIT, EncodingLimits, PUBLIC_KEY_LIMIT,
        import_evaluation_key, import_public_key,
    },
};

pub enum Progress {
    Waiting(ArtifactRef),
    Complete(ArtifactRef),
}

enum Step {
    Transfer {
        transfer: PreparedTransfer,
        prepared: Option<(PreparedBooleanMasks, [OwnerMaskContribution; 2])>,
    },
    EarlyMasks {
        source: NumberInput,
        masks: PreparedBooleanMasks,
    },
    Masks {
        number: TransferredNumber,
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

fn load_context(
    artifacts: &Artifacts,
    material: &super::KeyMaterial,
) -> Result<(EncryptionContext, EvaluationContext)> {
    let binding = material.binding()?;
    let limits = EncodingLimits::default();
    let public = import_public_key(
        &artifacts.read(&material.public_key, PUBLIC_KEY_LIMIT)?,
        binding,
        limits,
    )?;
    let evaluation = import_evaluation_key(
        &artifacts.read(&material.evaluation_key, EVALUATION_KEY_LIMIT)?,
        binding,
        limits,
    )?;
    Ok((
        EncryptionContext::new(public),
        EvaluationContext::new(evaluation),
    ))
}

impl ComparisonEngine {
    pub fn start(artifacts: Artifacts, manifest: CheckManifest) -> Result<(Self, Progress)> {
        if manifest.schema_version != arbiter_protocol::SCHEMA_VERSION {
            return Err(Error::Response);
        }
        if matches!(manifest.assurance, Assurance::ProofRequired) {
            return Err(Error::ProofUnavailable);
        }

        let width = records::width(manifest.width)?;
        let (source_encryption, source_evaluation) = load_context(&artifacts, &manifest.keys[0])?;
        let (target_encryption, target_evaluation) = load_context(&artifacts, &manifest.keys[1])?;
        let source = ComputationContext::new(source_encryption.clone(), source_evaluation)?;
        let target = ComputationContext::new(target_encryption.clone(), target_evaluation.clone())?;
        let x = NumberInput::import(
            &artifacts.read(&manifest.inputs[0], CIPHERTEXT_LIMIT)?,
            manifest.inputs[0].digest,
            width,
            &source_encryption,
        )?;
        let v = NumberInput::import(
            &artifacts.read(&manifest.inputs[1], CIPHERTEXT_LIMIT)?,
            manifest.inputs[1].digest,
            width,
            &target_encryption,
        )?;
        let owners = [
            OwnerId::new(*manifest.owners[0].as_bytes())?,
            OwnerId::new(*manifest.owners[1].as_bytes())?,
        ];
        let context = CheckContext::new(
            manifest.external_context,
            owners,
            manifest.comparison.operation(),
            &x,
            &v,
        )?;
        let record = ContextRecord {
            manifest,
            nonce: context.nonce(),
            native_inputs: [x.binding().native_digest(), v.binding().native_digest()],
        };
        let session = CheckSession::new(context, source, target)?;
        let mut engine = Self {
            artifacts,
            context: record,
            session,
            target: v,
            target_evaluation,
            step: None,
        };
        let progress = if engine.context.manifest.release_at_ms.is_some() {
            let masks = engine.session.prepare_boolean_masks()?;
            let request = Request::Masks {
                context: engine.context.clone(),
                base: masks.commitment(),
            };
            engine.step = Some(Step::EarlyMasks { source: x, masks });
            engine.wait(request)?
        } else {
            let transfer = engine.session.prepare_number_transfer(&x)?;
            engine.request_transfer(transfer, None)?
        };
        Ok((engine, progress))
    }

    pub fn advance(&mut self, response: Response) -> Result<Progress> {
        let step = self.step.take().ok_or(Error::Response)?;
        let expected = self.session.context();
        let context_id = expected.id();
        match (step, response) {
            (
                Step::Transfer { transfer, prepared },
                Response::Number {
                    context,
                    challenge,
                    ciphertext,
                },
            ) if context == context_id && challenge == transfer.challenge().id() => {
                let response = TransferResponse::import(
                    TransferBinding {
                        context,
                        reference: challenge,
                    },
                    &self.artifacts.read(&ciphertext, CIPHERTEXT_LIMIT)?,
                    ciphertext.digest,
                    transfer.challenge(),
                    expected,
                    &self.target_evaluation,
                )?;
                let number = self.session.finish_number_transfer(transfer, response)?;
                if let Some((masks, contributions)) = prepared {
                    return self.compare(number, masks, contributions);
                }
                let masks = self.session.prepare_boolean_masks()?;
                let request = Request::Masks {
                    context: self.context.clone(),
                    base: masks.commitment(),
                };
                self.step = Some(Step::Masks { number, masks });
                self.wait(request)
            }
            (
                Step::EarlyMasks { source, masks },
                Response::Masks {
                    context,
                    base,
                    contributions,
                },
            ) if context == context_id && base == masks.commitment() => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| Error::Response)?
                    .as_millis();
                let release = self.context.manifest.release_at_ms.ok_or(Error::Response)?;
                if release < 0 || now < release as u128 {
                    return Err(Error::Response);
                }
                let contributions = self.import_contributions(context, base, &contributions)?;
                let transfer = self.session.prepare_number_transfer(&source)?;
                self.request_transfer(transfer, Some((masks, contributions)))
            }
            (
                Step::Masks { number, masks },
                Response::Masks {
                    context,
                    base,
                    contributions,
                },
            ) if context == context_id && base == masks.commitment() => {
                let contributions = self.import_contributions(context, base, &contributions)?;
                self.compare(number, masks, contributions)
            }
            (
                Step::Commitment(comparison),
                Response::AnswerCommitment {
                    context,
                    challenge,
                    commitment,
                },
            ) if context == context_id && challenge == comparison.challenge().id() => {
                let request = Request::AnswerReveal {
                    context,
                    challenge,
                    commitment,
                };
                self.step = Some(Step::Answer {
                    comparison,
                    commitment: AnswerCommitment {
                        context,
                        challenge,
                        digest: commitment,
                    },
                });
                self.wait(request)
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
                let answer = MaskedBooleanAnswer::from_parts(context, challenge, value, expected)?;
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
                let open = |index: usize| -> Result<OwnerMaskOpening> {
                    let value = &openings[index];
                    Ok(OwnerMaskOpening::from_parts(
                        private_check::OwnerContributionBinding {
                            context,
                            base,
                            owner: OwnerId::new(*value.owner.as_bytes())?,
                            commitment: value.commitment,
                        },
                        value.value,
                        value.blinding,
                        expected,
                    )?)
                };
                let decoded = self.session.decode_boolean(
                    comparison,
                    commitment,
                    answer,
                    open(0)?,
                    open(1)?,
                )?;
                let result = self.artifacts.write_json(&serde_json::json!({
                    "schema_version": 1,
                    "context": context,
                    "value": decoded.value(),
                    "assurance": "honest_role_mechanics",
                    "proof_status": "unavailable",
                    "final_audit": "not_released",
                }))?;
                Ok(Progress::Complete(result))
            }
            _ => Err(Error::Response),
        }
    }

    fn wait(&self, request: Request) -> Result<Progress> {
        Ok(Progress::Waiting(self.artifacts.write_json(&request)?))
    }

    fn request_transfer(
        &mut self,
        transfer: PreparedTransfer,
        prepared: Option<(PreparedBooleanMasks, [OwnerMaskContribution; 2])>,
    ) -> Result<Progress> {
        let challenge = self
            .artifacts
            .write(transfer.challenge().encoded().as_bytes())?;
        let request = Request::Number {
            context: self.context.clone(),
            challenge,
            reference: transfer.challenge().id(),
        };
        self.step = Some(Step::Transfer { transfer, prepared });
        self.wait(request)
    }

    fn import_contributions(
        &self,
        context: [u8; 32],
        base: [u8; 32],
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
