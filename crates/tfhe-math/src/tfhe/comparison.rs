use super::value::NativeValue;
use crate::{MathError, Result, types::BinaryOp};
use ::tfhe::prelude::{FheEq, FheOrd};

pub(crate) fn evaluate(op: BinaryOp, a: &NativeValue, b: &NativeValue) -> Result<NativeValue> {
    macro_rules! cmp {
        ($a:expr, $b:expr) => {
            match op {
                BinaryOp::Eq => $a.eq($b),
                BinaryOp::Ne => $a.ne($b),
                BinaryOp::Lt => $a.lt($b),
                BinaryOp::Le => $a.le($b),
                BinaryOp::Gt => $a.gt($b),
                BinaryOp::Ge => $a.ge($b),
                _ => return Err(MathError::InvalidPlan),
            }
        };
    }

    Ok(NativeValue::Bool(match (a, b) {
        (NativeValue::Bool(a), NativeValue::Bool(b)) => match op {
            BinaryOp::Eq => a.eq(b),
            BinaryOp::Ne => a.ne(b),
            _ => return Err(MathError::InvalidPlan),
        },
        (NativeValue::U8(a), NativeValue::U8(b)) => cmp!(a, b),
        (NativeValue::U16(a), NativeValue::U16(b)) => cmp!(a, b),
        (NativeValue::U32(a), NativeValue::U32(b)) => cmp!(a, b),
        (NativeValue::U64(a), NativeValue::U64(b)) => cmp!(a, b),
        _ => return Err(MathError::InvalidPlan),
    }))
}
