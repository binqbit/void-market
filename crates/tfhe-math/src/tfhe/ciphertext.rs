//! Encryption, decryption and unpacked compact-input expansion.
use super::value::{NativeScalar, NativeValue};
use crate::{DataType, MathError, Result, Width};
use ::tfhe::prelude::{CiphertextList, FheDecrypt};

pub(crate) fn compact(
    key: &::tfhe::CompactPublicKey,
    value: NativeScalar,
) -> ::tfhe::CompactCiphertextList {
    let mut builder = ::tfhe::CompactCiphertextList::builder(key);
    match value {
        NativeScalar::Bool(value) => {
            builder.push(value);
        }
        NativeScalar::U8(value) => {
            builder.push(value);
        }
        NativeScalar::U16(value) => {
            builder.push(value);
        }
        NativeScalar::U32(value) => {
            builder.push(value);
        }
        NativeScalar::U64(value) => {
            builder.push(value);
        }
    }

    builder.build()
}

pub(crate) fn expand(list: &::tfhe::CompactCiphertextList, ty: DataType) -> Result<NativeValue> {
    let kind = match ty {
        DataType::Bool => ::tfhe::FheTypes::Bool,
        DataType::Uint(Width::U8) => ::tfhe::FheTypes::Uint8,
        DataType::Uint(Width::U16) => ::tfhe::FheTypes::Uint16,
        DataType::Uint(Width::U32) => ::tfhe::FheTypes::Uint32,
        DataType::Uint(Width::U64) => ::tfhe::FheTypes::Uint64,
    };

    if list.len() != 1 || list.get_kind_of(0) != Some(kind) {
        return Err(MathError::InvalidEncoding);
    }

    let expanded = list.expand().map_err(|_| MathError::InvalidEncoding)?;

    Ok(match ty {
        DataType::Bool => NativeValue::Bool(
            expanded
                .get(0)
                .map_err(|_| MathError::InvalidEncoding)?
                .ok_or(MathError::InvalidEncoding)?,
        ),
        DataType::Uint(Width::U8) => NativeValue::U8(
            expanded
                .get(0)
                .map_err(|_| MathError::InvalidEncoding)?
                .ok_or(MathError::InvalidEncoding)?,
        ),
        DataType::Uint(Width::U16) => NativeValue::U16(
            expanded
                .get(0)
                .map_err(|_| MathError::InvalidEncoding)?
                .ok_or(MathError::InvalidEncoding)?,
        ),
        DataType::Uint(Width::U32) => NativeValue::U32(
            expanded
                .get(0)
                .map_err(|_| MathError::InvalidEncoding)?
                .ok_or(MathError::InvalidEncoding)?,
        ),
        DataType::Uint(Width::U64) => NativeValue::U64(
            expanded
                .get(0)
                .map_err(|_| MathError::InvalidEncoding)?
                .ok_or(MathError::InvalidEncoding)?,
        ),
    })
}

pub(crate) fn encrypt(key: &::tfhe::CompactPublicKey, value: NativeScalar) -> Result<NativeValue> {
    expand(&compact(key, value), value.data_type())
}

pub(crate) fn decrypt(key: &::tfhe::ClientKey, value: &NativeValue) -> NativeScalar {
    match value {
        NativeValue::Bool(value) => NativeScalar::Bool(value.decrypt(key)),
        NativeValue::U8(value) => NativeScalar::U8(value.decrypt(key)),
        NativeValue::U16(value) => NativeScalar::U16(value.decrypt(key)),
        NativeValue::U32(value) => NativeScalar::U32(value.decrypt(key)),
        NativeValue::U64(value) => NativeScalar::U64(value.decrypt(key)),
    }
}
