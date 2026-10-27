use super::value::NativeValue;
use crate::{MathError, Result, types::BinaryOp};
use ::tfhe::prelude::{OverflowingAdd, OverflowingMul, OverflowingSub};

pub(crate) fn evaluate(
    op: BinaryOp,
    a: &NativeValue,
    b: &NativeValue,
) -> Result<(NativeValue, Option<NativeValue>)> {
    macro_rules! eval {
        ($x:expr, $y:expr, $variant:ident) => {{
            let (a, b) = ($x, $y);
            match op {
                BinaryOp::AddWide => (NativeValue::$variant(a + b), None),
                BinaryOp::MulWide => (NativeValue::$variant(a * b), None),
                BinaryOp::Add => {
                    let (v, f) = a.overflowing_add(b);
                    (NativeValue::$variant(v), Some(NativeValue::Bool(f)))
                }
                BinaryOp::Sub => {
                    let (v, f) = a.overflowing_sub(b);
                    (NativeValue::$variant(v), Some(NativeValue::Bool(f)))
                }
                BinaryOp::Mul => {
                    let (v, f) = a.overflowing_mul(b);
                    (NativeValue::$variant(v), Some(NativeValue::Bool(f)))
                }
                _ => return Err(MathError::InvalidPlan),
            }
        }};
    }

    Ok(match (a, b) {
        (NativeValue::U8(a), NativeValue::U8(b)) => eval!(a, b, U8),
        (NativeValue::U16(a), NativeValue::U16(b)) => eval!(a, b, U16),
        (NativeValue::U32(a), NativeValue::U32(b)) => eval!(a, b, U32),
        (NativeValue::U64(a), NativeValue::U64(b)) => eval!(a, b, U64),
        _ => return Err(MathError::InvalidPlan),
    })
}
