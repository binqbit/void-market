use crate::{DataType, Width};

#[derive(Clone)]
pub(crate) enum NativeValue {
    Bool(::tfhe::FheBool),
    U8(::tfhe::FheUint8),
    U16(::tfhe::FheUint16),
    U32(::tfhe::FheUint32),
    U64(::tfhe::FheUint64),
}

impl NativeValue {
    pub(crate) fn data_type(&self) -> DataType {
        match self {
            Self::Bool(_) => DataType::Bool,
            Self::U8(_) => DataType::Uint(Width::U8),
            Self::U16(_) => DataType::Uint(Width::U16),
            Self::U32(_) => DataType::Uint(Width::U32),
            Self::U64(_) => DataType::Uint(Width::U64),
        }
    }
}

/// Plain inputs and outputs matching the native supported TFHE types.
#[derive(Clone, Copy)]
pub(crate) enum NativeScalar {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
}

impl NativeScalar {
    pub(crate) fn data_type(self) -> DataType {
        match self {
            Self::Bool(_) => DataType::Bool,
            Self::U8(_) => DataType::Uint(Width::U8),
            Self::U16(_) => DataType::Uint(Width::U16),
            Self::U32(_) => DataType::Uint(Width::U32),
            Self::U64(_) => DataType::Uint(Width::U64),
        }
    }
}
