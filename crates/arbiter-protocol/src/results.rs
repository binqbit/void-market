use crate::{Completion, ProtocolError, SigningKey};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, VerifyingKey};

fn signed_bytes(value: &Completion) -> Vec<u8> {
    let mut e = minicbor::Encoder::new(Vec::new());
    e.array(11)
        .unwrap()
        .str("ARBITER_JOB_RESULT")
        .unwrap()
        .u8(1)
        .unwrap()
        .bytes(value.deployment_id.as_bytes())
        .unwrap()
        .bytes(value.node_id.as_bytes())
        .unwrap()
        .bytes(value.instance_id.as_bytes())
        .unwrap()
        .bytes(value.id.as_bytes())
        .unwrap()
        .bytes(value.job_id.as_bytes())
        .unwrap()
        .i64(value.generation)
        .unwrap()
        .str(value.status.as_str())
        .unwrap();
    if let Some(output) = &value.output {
        e.array(2)
            .unwrap()
            .bytes(output.id.as_bytes())
            .unwrap()
            .bytes(&output.digest)
            .unwrap();
    } else {
        e.null().unwrap();
    }
    if let Some(input) = value.consumed_response {
        e.bytes(input.as_bytes()).unwrap();
    } else {
        e.null().unwrap();
    }
    e.into_writer()
}

pub fn sign_result(value: &mut Completion, key: &SigningKey) {
    value.signature = URL_SAFE_NO_PAD.encode(key.sign(&signed_bytes(value)).to_bytes());
}

pub fn verify_result(value: &Completion, public: &str) -> Result<(), ProtocolError> {
    let public: [u8; 32] = URL_SAFE_NO_PAD
        .decode(public)
        .map_err(|_| ProtocolError::Authentication)?
        .try_into()
        .map_err(|_| ProtocolError::Authentication)?;
    let public = VerifyingKey::from_bytes(&public).map_err(|_| ProtocolError::Authentication)?;
    let signature = URL_SAFE_NO_PAD
        .decode(&value.signature)
        .map_err(|_| ProtocolError::Authentication)?;
    let signature = Signature::from_slice(&signature).map_err(|_| ProtocolError::Authentication)?;
    public
        .verify_strict(&signed_bytes(value), &signature)
        .map_err(|_| ProtocolError::Authentication)
}
