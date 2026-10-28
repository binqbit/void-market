use crate::{
    artifacts::Artifacts,
    error::{Error, Result},
};
use check_workflow::messages::{
    self as records, Assurance, CheckManifest, ContextRecord, KeyMaterial,
};
use private_check::{CheckContext, CheckSession, ComputationContext, NumberInput, OwnerId};
use tfhe_math::{
    EncryptionContext, EvaluationContext,
    codec::{
        CIPHERTEXT_LIMIT, EVALUATION_KEY_LIMIT, EncodingLimits, PUBLIC_KEY_LIMIT,
        import_evaluation_key, import_public_key,
    },
};

pub(super) struct Inputs {
    pub context: ContextRecord,
    pub session: CheckSession,
    pub target: NumberInput,
    pub target_evaluation: EvaluationContext,
    pub source: NumberInput,
}

pub(super) fn load(artifacts: &Artifacts, manifest: CheckManifest) -> Result<Inputs> {
    if manifest.schema_version != arbiter_protocol::SCHEMA_VERSION {
        return Err(Error::Response);
    }
    if matches!(manifest.assurance, Assurance::ProofRequired) {
        return Err(Error::ProofUnavailable);
    }

    let width = records::width(manifest.width)?;
    let (source_encryption, source_evaluation) = load_context(artifacts, &manifest.keys[0])?;
    let (target_encryption, target_evaluation) = load_context(artifacts, &manifest.keys[1])?;
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

    Ok(Inputs {
        context: record,
        session,
        target: v,
        target_evaluation,
        source: x,
    })
}

fn load_context(
    artifacts: &Artifacts,
    material: &KeyMaterial,
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
