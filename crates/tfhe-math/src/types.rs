use crate::{MathError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Width {
    U8 = 8,
    U16 = 16,
    U32 = 32,
    U64 = 64,
}

impl Width {
    pub const ALL: [Self; 4] = [Self::U8, Self::U16, Self::U32, Self::U64];
    pub const fn bits(self) -> u8 {
        self as u8
    }

    pub const fn maximum(self) -> u64 {
        u64::MAX >> (64 - self.bits())
    }

    pub fn from_bits(bits: u8) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|w| w.bits() == bits)
            .ok_or(MathError::UnsupportedWidth {
                required_bits: bits.into(),
            })
    }

    pub fn containing(bits: u16) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|w| u16::from(w.bits()) >= bits)
            .ok_or(MathError::UnsupportedWidth {
                required_bits: bits,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    Bool,
    Uint(Width),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Encrypted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    PublicReference,
    EncryptedTfhe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryOp {
    AddWide,
    MulWide,
    Add,
    Sub,
    Mul,
    Xor,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Not,
    Widen,
    BoolToUint,
    IsNonzero,
}
