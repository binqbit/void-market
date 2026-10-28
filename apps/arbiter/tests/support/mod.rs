use arbiter::{
    artifacts::Artifacts,
    crypto::{
        Assurance, CheckManifest, Contribution, KeyMaterial, Opening, Predicate, Request, Response,
    },
};
use private_check::{
    BooleanChallengeBinding, KeyOwnerContext, NumberInput, OwnerId, PendingBooleanAnswer,
    PreparedOwnerMask, TransferBinding, TransferChallenge,
};
use tfhe_math::{
    KeySet, Width,
    codec::{CIPHERTEXT_LIMIT, EncodingLimits, export_evaluation_key, export_public_key},
};
use uuid::Uuid;

pub(crate) struct Participants<'a> {
    keys: &'a [KeySet; 2],
    store: Artifacts,
    masks: Option<[PreparedOwnerMask; 2]>,
    answer: Option<PendingBooleanAnswer>,
    base: [u8; 32],
}
impl<'a> Participants<'a> {
    pub(crate) fn new(keys: &'a [KeySet; 2], store: Artifacts) -> Self {
        Self {
            keys,
            store,
            masks: None,
            answer: None,
            base: [0; 32],
        }
    }
    pub(crate) fn respond(&mut self, request: Request) -> Response {
        match request {
            Request::Number {
                context,
                challenge,
                reference,
            } => {
                let expected = context.restore().unwrap();
                let challenge = TransferChallenge::import(
                    TransferBinding {
                        context: expected.id(),
                        reference: expected.source().native_digest(),
                    },
                    &self.store.read(&challenge, CIPHERTEXT_LIMIT).unwrap(),
                    challenge.digest,
                    &expected,
                    &self.keys[0].evaluation_context(),
                )
                .unwrap();
                assert_eq!(challenge.id(), reference);
                let owner = KeyOwnerContext::new(
                    self.keys[0].decryption_context(),
                    self.keys[0].evaluation_context(),
                )
                .unwrap();
                let response = owner
                    .reencrypt_masked(&challenge, &self.keys[1].encryption_context(), &expected)
                    .unwrap();
                Response::Number {
                    context: expected.id(),
                    challenge: reference,
                    ciphertext: self.store.write(response.encoded().as_bytes()).unwrap(),
                }
            }
            Request::Masks { context, base } => {
                let expected = context.restore().unwrap();
                self.base = base;
                let first = PreparedOwnerMask::generate(
                    OwnerId::new(*context.manifest.owners[0].as_bytes()).unwrap(),
                    &expected,
                    base,
                    &self.keys[1].encryption_context(),
                )
                .unwrap();
                let second = PreparedOwnerMask::generate(
                    OwnerId::new(*context.manifest.owners[1].as_bytes()).unwrap(),
                    &expected,
                    base,
                    &self.keys[1].encryption_context(),
                )
                .unwrap();
                let contributions = [&first, &second].map(|value| {
                    let binding = value.contribution().binding();
                    Contribution {
                        owner: Uuid::from_bytes(*binding.owner.as_bytes()),
                        commitment: binding.commitment,
                        ciphertext: self
                            .store
                            .write(value.contribution().encoded().as_bytes())
                            .unwrap(),
                    }
                });
                self.masks = Some([first, second]);
                Response::Masks {
                    context: expected.id(),
                    base,
                    contributions,
                }
            }
            Request::AnswerCommitment {
                context,
                base,
                challenge,
                reference,
            } => {
                let expected = context.restore().unwrap();
                let owner = KeyOwnerContext::new(
                    self.keys[1].decryption_context(),
                    self.keys[1].evaluation_context(),
                )
                .unwrap();
                let encrypted = owner
                    .import_boolean_challenge(
                        BooleanChallengeBinding {
                            context: expected.id(),
                            base,
                        },
                        &self.store.read(&challenge, CIPHERTEXT_LIMIT).unwrap(),
                        challenge.digest,
                        &expected,
                        base,
                    )
                    .unwrap();
                assert_eq!(encrypted.id(), reference);
                let answer = owner
                    .prepare_boolean_answer(&encrypted, &expected, base)
                    .unwrap();
                let commitment = answer.commitment().digest;
                self.answer = Some(answer);
                Response::AnswerCommitment {
                    context: expected.id(),
                    challenge: reference,
                    commitment,
                }
            }
            Request::AnswerReveal {
                context,
                challenge,
                commitment,
            } => {
                let answer = self.answer.take().unwrap();
                assert_eq!(answer.commitment().digest, commitment);
                let revealed = answer.reveal(self.base).unwrap();
                Response::AnswerReveal {
                    context,
                    challenge,
                    value: revealed.value_for_protocol(),
                }
            }
            Request::Openings { context, base } => {
                let openings = self.masks.take().unwrap().map(|mask| {
                    let opening = mask.open();
                    let binding = opening.binding();
                    let (value, blinding) = opening.expose();
                    Opening {
                        owner: Uuid::from_bytes(*binding.owner.as_bytes()),
                        commitment: binding.commitment,
                        value,
                        blinding: *blinding,
                    }
                });
                Response::Openings {
                    context,
                    base,
                    openings,
                }
            }
        }
    }
}

pub(crate) fn prepare(
    store: &Artifacts,
    keys: &[KeySet; 2],
    values: [u64; 2],
    width: Width,
    comparison: Predicate,
) -> CheckManifest {
    let material = keys.each_ref().map(|key| KeyMaterial {
        configuration: key.binding().config_fingerprint(),
        public_fingerprint: key.binding().public_key_fingerprint(),
        evaluation_fingerprint: key.binding().evaluation_key_fingerprint(),
        public_key: store
            .write(
                export_public_key(key.public_key(), EncodingLimits::default())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
        evaluation_key: store
            .write(
                export_evaluation_key(key.evaluation_key(), EncodingLimits::default())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
    });
    let inputs = [
        NumberInput::encrypt(&keys[0].encryption_context(), width, values[0]).unwrap(),
        NumberInput::encrypt(&keys[1].encryption_context(), width, values[1]).unwrap(),
    ]
    .each_ref()
    .map(|input| store.write(input.encoded().as_bytes()).unwrap());
    CheckManifest {
        release_at_ms: None,
        schema_version: 1,
        assurance: Assurance::HonestRoleMechanics,
        external_context: [1; 32],
        owners: [Uuid::new_v4(), Uuid::new_v4()],
        width: width.bits(),
        comparison,
        keys: material,
        inputs,
    }
}
