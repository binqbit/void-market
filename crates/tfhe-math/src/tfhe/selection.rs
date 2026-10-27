use super::value::NativeValue;
use crate::{MathError, Result};
use ::tfhe::prelude::IfThenElse;

pub(crate) fn evaluate(c: &NativeValue, a: &NativeValue, b: &NativeValue) -> Result<NativeValue> {
    let NativeValue::Bool(c) = c else {
        return Err(MathError::InvalidPlan);
    };

    Ok(match (a, b) {
        (NativeValue::Bool(a), NativeValue::Bool(b)) => NativeValue::Bool(c.if_then_else(a, b)),
        (NativeValue::U8(a), NativeValue::U8(b)) => NativeValue::U8(c.if_then_else(a, b)),
        (NativeValue::U16(a), NativeValue::U16(b)) => NativeValue::U16(c.if_then_else(a, b)),
        (NativeValue::U32(a), NativeValue::U32(b)) => NativeValue::U32(c.if_then_else(a, b)),
        (NativeValue::U64(a), NativeValue::U64(b)) => NativeValue::U64(c.if_then_else(a, b)),
        _ => return Err(MathError::InvalidPlan),
    })
}
