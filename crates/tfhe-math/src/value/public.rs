#[cfg(feature = "tfhe-backend")]
use crate::tfhe::value::NativeScalar;
use crate::{DataType, MathError, Result, Width};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Literal {
    Bool(bool),
    Uint(u64),
}

/// A checked public literal with an explicit type; no implicit integer sizing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PublicValue {
    pub(crate) data_type: DataType,
    pub(crate) literal: Literal,
}

impl PublicValue {
    #[cfg(feature = "tfhe-backend")]
    pub(crate) fn to_native(self) -> Result<NativeScalar> {
        match (self.literal, self.data_type) {
            (Literal::Bool(value), DataType::Bool) => Ok(NativeScalar::Bool(value)),
            (Literal::Uint(value), DataType::Uint(width)) => Ok(match width {
                Width::U8 => NativeScalar::U8(value as u8),
                Width::U16 => NativeScalar::U16(value as u16),
                Width::U32 => NativeScalar::U32(value as u32),
                Width::U64 => NativeScalar::U64(value),
            }),
            _ => Err(MathError::InvalidPlan),
        }
    }

    #[cfg(feature = "tfhe-backend")]
    pub(crate) fn from_native(value: NativeScalar) -> Result<Self> {
        match value {
            NativeScalar::Bool(value) => Ok(Self::boolean(value)),
            NativeScalar::U8(value) => Self::uint(Width::U8, u64::from(value)),
            NativeScalar::U16(value) => Self::uint(Width::U16, u64::from(value)),
            NativeScalar::U32(value) => Self::uint(Width::U32, u64::from(value)),
            NativeScalar::U64(value) => Self::uint(Width::U64, value),
        }
    }

    pub const fn boolean(value: bool) -> Self {
        Self {
            data_type: DataType::Bool,
            literal: Literal::Bool(value),
        }
    }

    pub fn uint(width: Width, value: u64) -> Result<Self> {
        if value > width.maximum() {
            return Err(MathError::LiteralOutOfRange);
        }

        Ok(Self {
            data_type: DataType::Uint(width),
            literal: Literal::Uint(value),
        })
    }

    pub const fn data_type(&self) -> DataType {
        self.data_type
    }

    pub fn as_bool(&self) -> Result<bool> {
        match self.literal {
            Literal::Bool(v) => Ok(v),
            _ => Err(MathError::TypeMismatch),
        }
    }

    pub fn as_uint(&self) -> Result<u64> {
        match self.literal {
            Literal::Uint(v) => Ok(v),
            _ => Err(MathError::TypeMismatch),
        }
    }
}

impl std::fmt::Debug for PublicValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicValue")
            .field("data_type", &self.data_type)
            .finish_non_exhaustive()
    }
}
