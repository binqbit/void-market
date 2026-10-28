//! Bounded, versioned math artifacts. Fingerprints must be authenticated by the
//! application. Import establishes structural conformance, not plaintext validity.
//! Hostile native parsing must run under the host's OS memory/CPU containment;
//! a byte limit alone cannot guarantee an allocation bound in upstream TFHE.
use crate::tfhe::{self as native, key_scope};
use crate::{
    DataType, EncryptionContext, EvaluationContext, EvaluationKey, KeyBinding, MathError,
    PublicKey, PublicValue, Result, TfheValue, Width,
};
use sha2::{Digest, Sha256};

const MAGIC: &[u8; 8] = b"TFHEMATH";
const HEADER: usize = 148;
const CIPHERTEXT: u8 = 1;
const PUBLIC_KEY: u8 = 2;
const EVALUATION_KEY: u8 = 3;
const COMPACT_INPUT: u8 = 4;
pub const CIPHERTEXT_LIMIT: u64 = 16 * 1024 * 1024;
pub const PUBLIC_KEY_LIMIT: u64 = 16 * 1024 * 1024;
pub const EVALUATION_KEY_LIMIT: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct EncodingLimits {
    ciphertext: u64,
    public_key: u64,
    evaluation_key: u64,
}

impl Default for EncodingLimits {
    fn default() -> Self {
        Self {
            ciphertext: CIPHERTEXT_LIMIT,
            public_key: PUBLIC_KEY_LIMIT,
            evaluation_key: EVALUATION_KEY_LIMIT,
        }
    }
}

impl EncodingLimits {
    pub fn new(ciphertext: u64, public_key: u64, evaluation_key: u64) -> Result<Self> {
        for (n, max) in [
            (ciphertext, CIPHERTEXT_LIMIT),
            (public_key, PUBLIC_KEY_LIMIT),
            (evaluation_key, EVALUATION_KEY_LIMIT),
        ] {
            if n <= HEADER as u64 || n > max {
                return Err(MathError::ResourceLimit);
            }
        }

        Ok(Self {
            ciphertext,
            public_key,
            evaluation_key,
        })
    }

    fn cap(self, kind: u8) -> Result<u64> {
        match kind {
            CIPHERTEXT | COMPACT_INPUT => Ok(self.ciphertext),
            PUBLIC_KEY => Ok(self.public_key),
            EVALUATION_KEY => Ok(self.evaluation_key),
            _ => Err(MathError::InvalidEncoding),
        }
    }
}

/// Digest identifies these exact original frame bytes, not a reserialization.
#[derive(Clone)]
pub struct EncodedObject {
    bytes: Vec<u8>,
    digest: [u8; 32],
}

impl EncodedObject {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Exact safe-versioned native bytes, excluding this SDK's outer frame.
    /// Their identity differs from the digest of the complete encoded object.
    pub fn native_payload(&self) -> &[u8] {
        &self.bytes[HEADER..]
    }
}

impl std::fmt::Debug for EncodedObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncodedObject")
            .field("bytes", &self.bytes.len())
            .field("digest", &self.digest)
            .finish()
    }
}

fn type_code(ty: DataType) -> u8 {
    match ty {
        DataType::Bool => 1,
        DataType::Uint(Width::U8) => 2,
        DataType::Uint(Width::U16) => 3,
        DataType::Uint(Width::U32) => 4,
        DataType::Uint(Width::U64) => 5,
    }
}

fn encode_frame(
    kind: u8,
    ty: u8,
    binding: KeyBinding,
    body: Vec<u8>,
    limits: EncodingLimits,
) -> Result<EncodedObject> {
    let len = body
        .len()
        .checked_add(HEADER)
        .ok_or(MathError::ResourceLimit)?;
    if len as u64 > limits.cap(kind)? {
        return Err(MathError::ResourceLimit);
    }

    let mut bytes = Vec::with_capacity(len);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.push(kind);
    bytes.push(ty);
    bytes.extend_from_slice(&(body.len() as u64).to_le_bytes());

    for field in [
        binding.config,
        binding.public,
        binding.evaluation,
        binding.domain,
    ] {
        bytes.extend_from_slice(&field)
    }
    bytes.extend_from_slice(&body);

    Ok(EncodedObject {
        digest: Sha256::digest(&bytes).into(),
        bytes,
    })
}

fn parse_frame(
    bytes: &[u8],
    kind: u8,
    ty: u8,
    binding: KeyBinding,
    limits: EncodingLimits,
) -> Result<&[u8]> {
    if bytes.len() as u64 > limits.cap(kind)? {
        return Err(MathError::ResourceLimit);
    }

    if bytes.len() < HEADER
        || &bytes[..8] != MAGIC
        || bytes[8..10] != 1u16.to_le_bytes()
        || bytes[10] != kind
        || bytes[11] != ty
    {
        return Err(MathError::InvalidEncoding);
    }

    let length = u64::from_le_bytes(
        bytes[12..20]
            .try_into()
            .map_err(|_| MathError::InvalidEncoding)?,
    );
    if length != u64::try_from(bytes.len() - HEADER).map_err(|_| MathError::ResourceLimit)? {
        return Err(MathError::InvalidEncoding);
    }

    if bytes[20..52] != binding.config {
        return Err(MathError::ConfigMismatch);
    }
    if bytes[52..84] != binding.public
        || bytes[84..116] != binding.evaluation
        || bytes[116..148] != binding.domain
    {
        return Err(MathError::KeyDomainMismatch);
    }

    Ok(&bytes[HEADER..])
}

pub fn export_ciphertext(value: &TfheValue, limits: EncodingLimits) -> Result<EncodedObject> {
    let crate::value::Storage::Encrypted(v) = &value.storage else {
        return Err(MathError::BackendMismatch);
    };

    let body = key_scope::without_server_key(|| {
        native::serialization::serialize_value(&v.native, limits.ciphertext - HEADER as u64)
    })?;

    encode_frame(
        CIPHERTEXT,
        type_code(value.data_type()),
        v.binding,
        body,
        limits,
    )
}

/// `expected_digest` must be pinned by the surrounding transport/protocol. A
/// digest supplied by an untrusted peer is not authentication or a same-key proof.
pub fn import_ciphertext(
    bytes: &[u8],
    expected_digest: [u8; 32],
    expected_type: DataType,
    context: &EvaluationContext,
    limits: EncodingLimits,
) -> Result<TfheValue> {
    if bytes.len() as u64 > limits.ciphertext {
        return Err(MathError::ResourceLimit);
    }

    if <[u8; 32]>::from(Sha256::digest(bytes)) != expected_digest {
        return Err(MathError::InvalidEncoding);
    }

    let payload = parse_frame(
        bytes,
        CIPHERTEXT,
        type_code(expected_type),
        context.binding(),
        limits,
    )?;

    let value = key_scope::without_server_key(|| {
        native::serialization::deserialize_value(
            payload,
            expected_type,
            &context.key.native,
            limits.ciphertext - HEADER as u64,
        )
    })?;

    Ok(TfheValue::from_encrypted(value, context.binding()))
}

/// Import while retaining the authenticated original bytes for later binding.
pub fn import_ciphertext_artifact(
    bytes: &[u8],
    expected_digest: [u8; 32],
    expected_type: DataType,
    context: &EvaluationContext,
    limits: EncodingLimits,
) -> Result<(TfheValue, EncodedObject)> {
    let value = import_ciphertext(bytes, expected_digest, expected_type, context, limits)?;
    let original = EncodedObject {
        bytes: bytes.to_vec(),
        digest: expected_digest,
    };

    Ok((value, original))
}

pub fn export_public_key(key: &PublicKey, limits: EncodingLimits) -> Result<EncodedObject> {
    let body = native::serialization::serialize(&*key.native, limits.public_key - HEADER as u64)?;
    encode_frame(PUBLIC_KEY, 0, key.binding, body, limits)
}

pub fn import_public_key(
    bytes: &[u8],
    binding: KeyBinding,
    limits: EncodingLimits,
) -> Result<PublicKey> {
    let payload = parse_frame(bytes, PUBLIC_KEY, 0, binding, limits)?;
    if <[u8; 32]>::from(Sha256::digest(payload)) != binding.public {
        return Err(MathError::InvalidEncoding);
    }

    let key = key_scope::without_server_key(|| {
        native::serialization::deserialize_public_key(payload, limits.public_key - HEADER as u64)
    })?;

    Ok(PublicKey {
        native: std::sync::Arc::new(key),
        binding,
    })
}

pub fn export_evaluation_key(key: &EvaluationKey, limits: EncodingLimits) -> Result<EncodedObject> {
    let body =
        native::serialization::serialize(&*key.native, limits.evaluation_key - HEADER as u64)?;
    encode_frame(EVALUATION_KEY, 0, key.binding, body, limits)
}

pub fn import_evaluation_key(
    bytes: &[u8],
    binding: KeyBinding,
    limits: EncodingLimits,
) -> Result<EvaluationKey> {
    let payload = parse_frame(bytes, EVALUATION_KEY, 0, binding, limits)?;
    if <[u8; 32]>::from(Sha256::digest(payload)) != binding.evaluation {
        return Err(MathError::InvalidEncoding);
    }

    let key = key_scope::without_server_key(|| {
        native::serialization::deserialize_evaluation_key(
            payload,
            limits.evaluation_key - HEADER as u64,
        )
    })?;

    Ok(EvaluationKey {
        native: std::sync::Arc::new(key),
        binding,
    })
}

/// Encrypt exactly one unpacked input, retaining the exact compact wire bytes.
pub fn encrypt_compact(
    context: &EncryptionContext,
    value: PublicValue,
    limits: EncodingLimits,
) -> Result<EncodedObject> {
    let payload = key_scope::without_server_key(|| {
        native::serialization::serialize(
            &native::ciphertext::compact(&context.key.native, value.to_native()?),
            limits.ciphertext - HEADER as u64,
        )
    })?;

    encode_frame(
        COMPACT_INPUT,
        type_code(value.data_type()),
        context.binding(),
        payload,
        limits,
    )
}

/// Import one unpacked, no-cast compact input. Packed inputs are rejected with
/// the thread's server key cleared; there is deliberately no keyed retry.
pub fn import_compact(
    bytes: &[u8],
    expected_digest: [u8; 32],
    expected_type: DataType,
    context: &EncryptionContext,
    limits: EncodingLimits,
) -> Result<TfheValue> {
    if bytes.len() as u64 > limits.ciphertext {
        return Err(MathError::ResourceLimit);
    }

    if <[u8; 32]>::from(Sha256::digest(bytes)) != expected_digest {
        return Err(MathError::InvalidEncoding);
    }

    let payload = parse_frame(
        bytes,
        COMPACT_INPUT,
        type_code(expected_type),
        context.binding(),
        limits,
    )?;

    let value = key_scope::without_server_key(|| {
        let list = native::serialization::deserialize_compact(
            payload,
            &context.key.native,
            limits.ciphertext - HEADER as u64,
        )?;

        native::ciphertext::expand(&list, expected_type)
    })?;

    Ok(TfheValue::from_encrypted(value, context.binding()))
}

/// Import one compact input while preserving its original frame and native bytes.
pub fn import_compact_artifact(
    bytes: &[u8],
    expected_digest: [u8; 32],
    expected_type: DataType,
    context: &EncryptionContext,
    limits: EncodingLimits,
) -> Result<(TfheValue, EncodedObject)> {
    let value = import_compact(bytes, expected_digest, expected_type, context, limits)?;
    let original = EncodedObject {
        bytes: bytes.to_vec(),
        digest: expected_digest,
    };

    Ok((value, original))
}
