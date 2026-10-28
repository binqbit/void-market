use crate::{
    http::error::{ApiError, Result},
    state::AppState,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::signature::Ed25519KeyPair;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

pub struct Binding<'a> {
    pub account: Uuid,
    pub session: Uuid,
    pub action: Uuid,
    pub request: Uuid,
    pub revision: i64,
    pub digest: [u8; 32],
    pub purpose: &'a str,
    pub received_ms: i64,
}

pub async fn sign(
    state: &AppState,
    connection: &mut PgConnection,
    binding: Binding<'_>,
) -> Result<Value> {
    let Binding {
        account,
        session,
        action,
        request,
        revision,
        digest,
        purpose,
        received_ms,
    } = binding;
    let custody = state.custody.as_ref().ok_or(ApiError::Unavailable)?;
    let (sealed, public): (Vec<u8>, String) = sqlx::query_as(
        "SELECT signing_key, signing_public_key FROM custody_keys WHERE account_id=$1",
    )
    .bind(account)
    .fetch_one(connection)
    .await?;
    let seed = custody.vault.open(account, "signing-key", &sealed)?;
    let key = Ed25519KeyPair::from_seed_unchecked(&seed).map_err(|_| ApiError::Internal)?;
    let mut encoded = minicbor::Encoder::new(Vec::new());
    encoded
        .array(10)
        .map_err(|_| ApiError::Internal)?
        .str("CUSTODIAL_USER_ACTION")
        .map_err(|_| ApiError::Internal)?
        .u8(1)
        .map_err(|_| ApiError::Internal)?
        .bytes(account.as_bytes())
        .map_err(|_| ApiError::Internal)?
        .bytes(session.as_bytes())
        .map_err(|_| ApiError::Internal)?
        .bytes(action.as_bytes())
        .map_err(|_| ApiError::Internal)?
        .bytes(request.as_bytes())
        .map_err(|_| ApiError::Internal)?
        .i64(revision)
        .map_err(|_| ApiError::Internal)?
        .bytes(&digest)
        .map_err(|_| ApiError::Internal)?
        .str(purpose)
        .map_err(|_| ApiError::Internal)?
        .i64(received_ms)
        .map_err(|_| ApiError::Internal)?;
    let bytes = encoded.into_writer();
    let signature = key.sign(&bytes);
    Ok(json!({
        "mode":"dev_custodial",
        "public_key":public,
        "revision":revision,
        "received_ms":received_ms,
        "payload":URL_SAFE_NO_PAD.encode(bytes),
        "signature":URL_SAFE_NO_PAD.encode(signature.as_ref())
    }))
}

pub fn verify(receipt: &Value, expected: &Binding<'_>) -> Result<()> {
    let text = |name| {
        receipt
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ApiError::Forbidden)
    };
    let public = URL_SAFE_NO_PAD
        .decode(text("public_key")?)
        .map_err(|_| ApiError::Forbidden)?;
    let payload = URL_SAFE_NO_PAD
        .decode(text("payload")?)
        .map_err(|_| ApiError::Forbidden)?;
    let signature = URL_SAFE_NO_PAD
        .decode(text("signature")?)
        .map_err(|_| ApiError::Forbidden)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(&payload, &signature)
        .map_err(|_| ApiError::Forbidden)?;
    let mut decoder = minicbor::Decoder::new(&payload);
    let valid = (|| {
        Ok::<_, minicbor::decode::Error>(
            decoder.array()? == Some(10)
                && decoder.str()? == "CUSTODIAL_USER_ACTION"
                && decoder.u8()? == 1
                && decoder.bytes()? == expected.account.as_bytes()
                && decoder.bytes()? == expected.session.as_bytes()
                && decoder.bytes()? == expected.action.as_bytes()
                && decoder.bytes()? == expected.request.as_bytes()
                && decoder.i64()? == expected.revision
                && decoder.bytes()? == expected.digest
                && decoder.str()? == expected.purpose
                && decoder.i64()? == expected.received_ms
                && decoder.position() == payload.len(),
        )
    })()
    .map_err(|_| ApiError::Forbidden)?;
    if !valid {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
