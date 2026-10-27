use super::{parameters, value::NativeValue};
use crate::{DataType, MathError, Result, Width};
use ::tfhe::{Unversionize, Versionize, conformance::ParameterSetConformant, named::Named};
use std::io::Cursor;

pub(crate) fn serialize<T: serde::Serialize + Versionize + Named>(
    value: &T,
    limit: u64,
) -> Result<Vec<u8>> {
    let size = ::tfhe::safe_serialization::safe_serialized_size(value)
        .map_err(|_| MathError::InvalidEncoding)?;
    if size > limit {
        return Err(MathError::ResourceLimit);
    }

    let mut bytes = Vec::new();
    ::tfhe::safe_serialization::safe_serialize(value, &mut bytes, limit)
        .map_err(|_| MathError::InvalidEncoding)?;
    if bytes.len() as u64 > limit {
        return Err(MathError::ResourceLimit);
    }

    Ok(bytes)
}

fn deserialize<T: serde::de::DeserializeOwned + Unversionize + Named + ParameterSetConformant>(
    bytes: &[u8],
    params: &T::ParameterSet,
    limit: u64,
) -> Result<T> {
    if bytes.len() as u64 > limit {
        return Err(MathError::ResourceLimit);
    }

    let mut reader = Cursor::new(bytes);
    let value = ::tfhe::safe_serialization::safe_deserialize_conformant(&mut reader, limit, params)
        .map_err(|_| MathError::InvalidEncoding)?;

    if reader.position() != bytes.len() as u64 {
        return Err(MathError::InvalidEncoding);
    }

    Ok(value)
}

pub(crate) fn serialize_value(value: &NativeValue, limit: u64) -> Result<Vec<u8>> {
    match value {
        NativeValue::Bool(v) => serialize(v, limit),
        NativeValue::U8(v) => serialize(v, limit),
        NativeValue::U16(v) => serialize(v, limit),
        NativeValue::U32(v) => serialize(v, limit),
        NativeValue::U64(v) => serialize(v, limit),
    }
}

pub(crate) fn deserialize_value(
    bytes: &[u8],
    ty: DataType,
    key: &::tfhe::ServerKey,
    limit: u64,
) -> Result<NativeValue> {
    macro_rules! get {
        ($ty:ty, $params:ty, $variant:ident) => {
            Ok(NativeValue::$variant(deserialize::<$ty>(
                bytes,
                &<$params>::from(key),
                limit,
            )?))
        };
    }

    match ty {
        DataType::Bool => get!(::tfhe::FheBool, ::tfhe::FheBoolConformanceParams, Bool),
        DataType::Uint(Width::U8) => get!(::tfhe::FheUint8, ::tfhe::FheUint8ConformanceParams, U8),
        DataType::Uint(Width::U16) => {
            get!(::tfhe::FheUint16, ::tfhe::FheUint16ConformanceParams, U16)
        }
        DataType::Uint(Width::U32) => {
            get!(::tfhe::FheUint32, ::tfhe::FheUint32ConformanceParams, U32)
        }
        DataType::Uint(Width::U64) => {
            get!(::tfhe::FheUint64, ::tfhe::FheUint64ConformanceParams, U64)
        }
    }
}

pub(crate) fn deserialize_public_key(bytes: &[u8], limit: u64) -> Result<::tfhe::CompactPublicKey> {
    let params = parameters::config()
        .public_key_encryption_parameters()
        .map_err(|_| MathError::ConfigMismatch)?;

    deserialize(bytes, &params, limit)
}

pub(crate) fn deserialize_evaluation_key(bytes: &[u8], limit: u64) -> Result<::tfhe::ServerKey> {
    let params: <::tfhe::ServerKey as ParameterSetConformant>::ParameterSet =
        parameters::config().into();

    deserialize(bytes, &params, limit)
}

pub(crate) fn deserialize_compact(
    bytes: &[u8],
    key: &::tfhe::CompactPublicKey,
    limit: u64,
) -> Result<::tfhe::CompactCiphertextList> {
    let params =
        ::tfhe::CompactCiphertextListConformanceParams::from_parameters_and_size_constraint(
            key.parameters(),
            ::tfhe::conformance::ListSizeConstraint::exact_size(1),
        )
        .allow_unpacked();

    deserialize(bytes, &params, limit)
}
