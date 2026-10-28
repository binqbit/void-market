use crate::ProtocolError;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
pub use ed25519_dalek::SigningKey;
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSignature {
    pub deployment_id: Uuid,
    pub node_id: Uuid,
    pub instance_id: Uuid,
    pub request_id: Uuid,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub signature: String,
}

pub fn public_key(key: &SigningKey) -> String {
    URL_SAFE_NO_PAD.encode(key.verifying_key().as_bytes())
}

fn signed_bytes(request: &RequestSignature, method: &str, target: &str, body: &[u8]) -> Vec<u8> {
    let mut encoder = minicbor::Encoder::new(Vec::new());
    encoder
        .array(11)
        .expect("Vec encoding is infallible")
        .str("ARBITER_HTTP_REQUEST")
        .unwrap()
        .u8(1)
        .unwrap()
        .bytes(request.deployment_id.as_bytes())
        .unwrap()
        .bytes(request.node_id.as_bytes())
        .unwrap()
        .bytes(request.instance_id.as_bytes())
        .unwrap()
        .bytes(request.request_id.as_bytes())
        .unwrap()
        .i64(request.issued_at_ms)
        .unwrap()
        .i64(request.expires_at_ms)
        .unwrap()
        .str(method)
        .unwrap()
        .str(target)
        .unwrap()
        .bytes(&Sha256::digest(body))
        .unwrap();
    encoder.into_writer()
}

pub fn sign(
    key: &SigningKey,
    request: &mut RequestSignature,
    method: &str,
    target: &str,
    body: &[u8],
) {
    request.signature = URL_SAFE_NO_PAD.encode(
        key.sign(&signed_bytes(request, method, target, body))
            .to_bytes(),
    );
}

pub fn verify(
    public: &str,
    request: &RequestSignature,
    method: &str,
    target: &str,
    body: &[u8],
    now_ms: i64,
) -> Result<(), ProtocolError> {
    if request.issued_at_ms > now_ms + 5_000
        || request.expires_at_ms <= now_ms
        || request.expires_at_ms <= request.issued_at_ms
        || !request
            .expires_at_ms
            .checked_sub(request.issued_at_ms)
            .is_some_and(|duration| (1..=60_000).contains(&duration))
        || !matches!(method, "GET" | "POST")
        || !target.starts_with("/node/v1/")
        || target.contains(['%', '?', '#'])
        || target.contains("..")
        || target.contains("//")
        || body.len() > crate::BODY_LIMIT
    {
        return Err(ProtocolError::Authentication);
    }

    let bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(public)
        .map_err(|_| ProtocolError::Authentication)?
        .try_into()
        .map_err(|_| ProtocolError::Authentication)?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| ProtocolError::Authentication)?;
    let signature = URL_SAFE_NO_PAD
        .decode(&request.signature)
        .map_err(|_| ProtocolError::Authentication)?;
    let signature = Signature::from_slice(&signature).map_err(|_| ProtocolError::Authentication)?;
    key.verify_strict(&signed_bytes(request, method, target, body), &signature)
        .map_err(|_| ProtocolError::Authentication)
}
