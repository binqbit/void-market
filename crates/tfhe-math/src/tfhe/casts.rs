use super::value::{NativeScalar, NativeValue};
use crate::{DataType, MathError, Result, Width};
use ::tfhe::prelude::{CastFrom, FheEq, FheTrivialEncrypt};

pub(crate) fn trivial(value: NativeScalar) -> NativeValue {
    match value {
        NativeScalar::Bool(value) => NativeValue::Bool(::tfhe::FheBool::encrypt_trivial(value)),
        NativeScalar::U8(value) => NativeValue::U8(::tfhe::FheUint8::encrypt_trivial(value)),
        NativeScalar::U16(value) => NativeValue::U16(::tfhe::FheUint16::encrypt_trivial(value)),
        NativeScalar::U32(value) => NativeValue::U32(::tfhe::FheUint32::encrypt_trivial(value)),
        NativeScalar::U64(value) => NativeValue::U64(::tfhe::FheUint64::encrypt_trivial(value)),
    }
}

pub(crate) fn cast(v: &NativeValue, output: DataType) -> Result<NativeValue> {
    if v.data_type() == output {
        return Ok(v.clone());
    }
    macro_rules! cast_to {
        ($ty:ty, $variant:ident) => {
            Ok(NativeValue::$variant(match v {
                NativeValue::Bool(v) => <$ty>::cast_from(v.clone()),
                NativeValue::U8(v) => <$ty>::cast_from(v.clone()),
                NativeValue::U16(v) => <$ty>::cast_from(v.clone()),
                NativeValue::U32(v) => <$ty>::cast_from(v.clone()),
                NativeValue::U64(v) => <$ty>::cast_from(v.clone()),
            }))
        };
    }

    match output {
        DataType::Bool => Err(MathError::InvalidPlan),
        DataType::Uint(Width::U8) => cast_to!(::tfhe::FheUint8, U8),
        DataType::Uint(Width::U16) => cast_to!(::tfhe::FheUint16, U16),
        DataType::Uint(Width::U32) => cast_to!(::tfhe::FheUint32, U32),
        DataType::Uint(Width::U64) => cast_to!(::tfhe::FheUint64, U64),
    }
}

pub(crate) fn is_nonzero(v: &NativeValue) -> Result<NativeValue> {
    Ok(NativeValue::Bool(match v {
        NativeValue::U8(v) => v.ne(0u8),
        NativeValue::U16(v) => v.ne(0u16),
        NativeValue::U32(v) => v.ne(0u32),
        NativeValue::U64(v) => v.ne(0u64),
        _ => return Err(MathError::InvalidPlan),
    }))
}
