use crate::{CheckContext, CheckError, Digest, OwnerId, Result};
use minicbor::Encoder;
use sha2::{Digest as HashDigest, Sha256};
use tfhe_math::Width;
use zeroize::Zeroizing;

type Encoding = Encoder<Vec<u8>>;

fn digest(encode: impl FnOnce(&mut Encoding) -> Result<()>) -> Result<Digest> {
    let mut encoder = Encoder::new(Vec::new());
    encode(&mut encoder)?;
    let bytes = Zeroizing::new(encoder.into_writer());
    Ok(Sha256::digest(bytes.as_slice()).into())
}

fn bytes(encoder: &mut Encoding, value: &[u8]) -> Result<()> {
    encoder.bytes(value).map_err(|_| CheckError::Encoding)?;
    Ok(())
}

fn uint(encoder: &mut Encoding, value: u64) -> Result<()> {
    encoder.u64(value).map_err(|_| CheckError::Encoding)?;
    Ok(())
}

fn array(encoder: &mut Encoding, length: u64) -> Result<()> {
    encoder.array(length).map_err(|_| CheckError::Encoding)?;
    Ok(())
}

fn tag(encoder: &mut Encoding, value: &str) -> Result<()> {
    encoder.str(value).map_err(|_| CheckError::Encoding)?;
    Ok(())
}

fn key(encoder: &mut Encoding, binding: tfhe_math::KeyBinding) -> Result<()> {
    array(encoder, 4)?;
    bytes(encoder, &binding.config_fingerprint())?;
    bytes(encoder, &binding.public_key_fingerprint())?;
    bytes(encoder, &binding.evaluation_key_fingerprint())?;
    bytes(encoder, &binding.domain_id())
}

pub(crate) fn context_digest(context: &CheckContext) -> Result<Digest> {
    digest(|e| {
        array(e, 14)?;
        tag(e, "PRIVATE_CHECK_CONTEXT_V1")?;
        tag(e, "AUTHOR_SPLIT_MASK_WIDTHS_V3")?;
        bytes(e, &context.external)?;
        bytes(e, &context.nonce)?;
        uint(e, context.width().bits().into())?;
        uint(e, context.comparison as u64)?;
        bytes(e, context.owners[0].as_bytes())?;
        bytes(e, context.owners[1].as_bytes())?;
        for input in [context.source, context.target] {
            bytes(e, &input.frame)?;
            bytes(e, &input.native)?;
            key(e, input.key)?;
        }
        Ok(())
    })
}

pub(crate) fn owner(
    context: Digest,
    width: Width,
    owner: OwnerId,
    pad: u64,
    blinding: &[u8; 32],
) -> Result<Digest> {
    digest(|e| {
        array(e, 5)?;
        tag(e, &format!("USER_MASK_ADD_XOR{}_V1", width.bits()))?;
        bytes(e, &context)?;
        bytes(e, owner.as_bytes())?;
        uint(e, pad)?;
        bytes(e, blinding)
    })
}

pub(crate) fn base(context: Digest, width: Width, ciphertexts: [&[u8]; 3]) -> Result<Digest> {
    digest(|e| {
        array(e, 5)?;
        tag(e, &format!("BASE_MASKS_BOOL_SPACED_XOR{}_V3", width.bits()))?;
        bytes(e, &context)?;
        for ciphertext in ciphertexts {
            bytes(e, ciphertext)?;
        }
        Ok(())
    })
}

pub(crate) fn answer(context: Digest, width: Width, value: u64) -> Result<Digest> {
    digest(|e| {
        array(e, 3)?;
        tag(
            e,
            &format!("MASKED_ANSWER_BOOL_SPACED_XOR{}_V3", width.bits()),
        )?;
        bytes(e, &context)?;
        uint(e, value)
    })
}

pub(crate) fn message_reference(context: Digest, base: Digest, output: Digest) -> Result<Digest> {
    digest(|e| {
        array(e, 4)?;
        tag(e, "PRIVATE_CHECK_BOOLEAN_CHALLENGE_V1")?;
        bytes(e, &context)?;
        bytes(e, &base)?;
        bytes(e, &output)
    })
}

pub(crate) fn transfer_reference(context: Digest, input: Digest, output: Digest) -> Result<Digest> {
    digest(|e| {
        array(e, 4)?;
        tag(e, "PRIVATE_CHECK_TRANSFER_CHALLENGE_V1")?;
        bytes(e, &context)?;
        bytes(e, &input)?;
        bytes(e, &output)
    })
}

#[cfg(test)]
#[path = "../../tests/unit/commitments.rs"]
mod tests;
