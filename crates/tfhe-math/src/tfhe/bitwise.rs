use super::value::NativeValue;
use crate::{MathError, Result, types::BinaryOp};

pub(crate) fn binary(op: BinaryOp, a: &NativeValue, b: &NativeValue) -> Result<NativeValue> {
    macro_rules! eval {
        ($a:expr, $b:expr, $v:ident) => {
            NativeValue::$v(match op {
                BinaryOp::Xor => $a ^ $b,
                BinaryOp::And => $a & $b,
                BinaryOp::Or => $a | $b,
                _ => return Err(MathError::InvalidPlan),
            })
        };
    }

    Ok(match (a, b) {
        (NativeValue::Bool(a), NativeValue::Bool(b)) => eval!(a, b, Bool),
        (NativeValue::U8(a), NativeValue::U8(b)) => eval!(a, b, U8),
        (NativeValue::U16(a), NativeValue::U16(b)) => eval!(a, b, U16),
        (NativeValue::U32(a), NativeValue::U32(b)) => eval!(a, b, U32),
        (NativeValue::U64(a), NativeValue::U64(b)) => eval!(a, b, U64),
        _ => return Err(MathError::InvalidPlan),
    })
}

pub(crate) fn not(v: &NativeValue) -> NativeValue {
    match v {
        NativeValue::Bool(v) => NativeValue::Bool(!v),
        NativeValue::U8(v) => NativeValue::U8(!v),
        NativeValue::U16(v) => NativeValue::U16(!v),
        NativeValue::U32(v) => NativeValue::U32(!v),
        NativeValue::U64(v) => NativeValue::U64(!v),
    }
}
