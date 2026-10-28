use crate::http::error::{ApiError, Result};
use num_bigint::BigUint;
use sha3::{Digest, Keccak256};

pub fn address(value: &str) -> Result<String> {
    let value = value
        .strip_prefix("0x")
        .ok_or(ApiError::BadRequest("Invalid Ethereum address"))?;
    if value.len() != 40
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value.bytes().all(|byte| byte == b'0')
    {
        return Err(ApiError::BadRequest("Invalid Ethereum address"));
    }
    Ok(format!("0x{}", value.to_ascii_lowercase()))
}

pub fn checked_address(value: &str) -> Result<String> {
    let normalized = address(value)?;
    let digits = &value[2..];
    let mixed = digits.bytes().any(|byte| byte.is_ascii_lowercase())
        && digits.bytes().any(|byte| byte.is_ascii_uppercase());
    if mixed {
        let digest = hex::encode(Keccak256::digest(&normalized.as_bytes()[2..]));
        for (byte, hash) in digits.bytes().zip(digest.bytes()) {
            if byte.is_ascii_alphabetic() && byte.is_ascii_uppercase() != (hash >= b'8') {
                return Err(ApiError::BadRequest("Ethereum address checksum is invalid"));
            }
        }
    }
    Ok(normalized)
}

pub fn transaction_hash(value: &str) -> Result<String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or(ApiError::BadRequest("Invalid transaction hash"))?;
    if digits.len() != 64 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest("Invalid transaction hash"));
    }
    Ok(format!("0x{}", digits.to_ascii_lowercase()))
}

pub fn quantity(value: &str) -> Result<u128> {
    let value = value.strip_prefix("0x").ok_or(ApiError::Unavailable)?;
    u128::from_str_radix(value, 16).map_err(|_| ApiError::Unavailable)
}

pub fn transaction_id(raw: &str) -> Result<String> {
    let bytes = hex::decode(raw.strip_prefix("0x").ok_or(ApiError::Unavailable)?)
        .map_err(|_| ApiError::Unavailable)?;
    Ok(format!("0x{}", hex::encode(Keccak256::digest(bytes))))
}

fn word_address(value: &str) -> Result<String> {
    Ok(format!("{:0>64}", &address(value)?[2..]))
}

pub fn token(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 78 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::BadRequest("Invalid NFT token ID"));
    }
    let id = BigUint::parse_bytes(value.as_bytes(), 10)
        .ok_or(ApiError::BadRequest("Invalid NFT token ID"))?;
    if id.bits() > 256 {
        return Err(ApiError::BadRequest("NFT token ID exceeds uint256"));
    }
    if value != id.to_str_radix(10) {
        return Err(ApiError::BadRequest("Use a canonical NFT token ID"));
    }
    Ok(format!("{:0>64}", id.to_str_radix(16)))
}

pub fn owner_of(id: &str) -> Result<String> {
    Ok(format!("0x6352211e{}", token(id)?))
}

pub fn transfer(from: &str, to: &str, id: &str) -> Result<String> {
    Ok(format!(
        "0x42842e0e{}{}{}",
        word_address(from)?,
        word_address(to)?,
        token(id)?
    ))
}

pub fn mint(to: &str, id: &str, uri: &str) -> Result<String> {
    if uri.len() > 2048 {
        return Err(ApiError::BadRequest("NFT metadata URI is too long"));
    }
    let encoded = hex::encode(uri.as_bytes());
    let padded = encoded.len().div_ceil(64) * 64;
    Ok(format!(
        "0xd3fc9864{}{}{:064x}{:064x}{:0<width$}",
        word_address(to)?,
        token(id)?,
        96,
        uri.len(),
        encoded,
        width = padded,
    ))
}

pub fn decoded_owner(output: &str) -> Result<String> {
    let value = output.strip_prefix("0x").ok_or(ApiError::Unavailable)?;
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value[..24].bytes().any(|byte| byte != b'0')
    {
        return Err(ApiError::Unavailable);
    }
    address(&format!("0x{}", &value[24..]))
}
