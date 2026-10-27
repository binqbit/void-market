use super::{BinaryResult, OperationExecutor};
use crate::{
    DataType, MathError, PublicValue, Result, TfheValue, Width,
    types::{BinaryOp, UnaryOp},
    value::Literal,
    width_policy::BinaryPlan,
};

pub(crate) struct PublicReference;
fn uint(width: Width, v: u64) -> Result<TfheValue> {
    Ok(PublicValue::uint(width, v)?.into())
}

fn boolean(v: bool) -> TfheValue {
    PublicValue::boolean(v).into()
}

impl OperationExecutor for PublicReference {
    fn binary(&self, plan: BinaryPlan, lhs: &TfheValue, rhs: &TfheValue) -> Result<BinaryResult> {
        let a = lhs.public_value()?;
        let b = rhs.public_value()?;

        let mut flag = None;
        let value = match (a.literal, b.literal, plan.operands) {
            (Literal::Uint(a), Literal::Uint(b), DataType::Uint(w)) => {
                let wide = match plan.op {
                    BinaryOp::AddWide | BinaryOp::Add => u128::from(a) + u128::from(b),
                    BinaryOp::MulWide | BinaryOp::Mul => u128::from(a) * u128::from(b),
                    _ => 0,
                };

                match plan.op {
                    BinaryOp::AddWide | BinaryOp::MulWide | BinaryOp::Add | BinaryOp::Mul => {
                        if plan.flagged {
                            flag = Some(boolean(wide > u128::from(w.maximum())));
                        }
                        uint(w, (wide & u128::from(w.maximum())) as u64)?
                    }
                    BinaryOp::Sub => {
                        flag = Some(boolean(a < b));
                        uint(w, a.wrapping_sub(b) & w.maximum())?
                    }
                    BinaryOp::Xor => uint(w, a ^ b)?,
                    BinaryOp::And => uint(w, a & b)?,
                    BinaryOp::Or => uint(w, a | b)?,
                    BinaryOp::Eq => boolean(a == b),
                    BinaryOp::Ne => boolean(a != b),
                    BinaryOp::Lt => boolean(a < b),
                    BinaryOp::Le => boolean(a <= b),
                    BinaryOp::Gt => boolean(a > b),
                    BinaryOp::Ge => boolean(a >= b),
                }
            }
            (Literal::Bool(a), Literal::Bool(b), DataType::Bool) => match plan.op {
                BinaryOp::Xor => boolean(a ^ b),
                BinaryOp::And => boolean(a & b),
                BinaryOp::Or => boolean(a | b),
                BinaryOp::Eq => boolean(a == b),
                BinaryOp::Ne => boolean(a != b),
                _ => return Err(MathError::InvalidPlan),
            },
            _ => return Err(MathError::InvalidPlan),
        };

        if value.data_type() != plan.output {
            return Err(MathError::InvalidPlan);
        }

        Ok(BinaryResult { value, flag })
    }

    fn unary(&self, op: UnaryOp, output: DataType, value: &TfheValue) -> Result<TfheValue> {
        let v = value.public_value()?;

        match (op, v.literal, output) {
            (UnaryOp::Not, Literal::Bool(b), DataType::Bool) => Ok(boolean(!b)),
            (UnaryOp::Not, Literal::Uint(n), DataType::Uint(w)) => uint(w, !n & w.maximum()),
            (UnaryOp::Widen, Literal::Uint(n), DataType::Uint(w)) => uint(w, n),
            (UnaryOp::BoolToUint, Literal::Bool(b), DataType::Uint(w)) => uint(w, u64::from(b)),
            (UnaryOp::IsNonzero, Literal::Uint(n), DataType::Bool) => Ok(boolean(n != 0)),
            _ => Err(MathError::InvalidPlan),
        }
    }

    fn select(
        &self,
        output: DataType,
        condition: &TfheValue,
        yes: &TfheValue,
        no: &TfheValue,
    ) -> Result<TfheValue> {
        let c = condition.public_value()?.as_bool()?;
        let chosen = if c { yes } else { no };

        match output {
            DataType::Bool => Ok(boolean(chosen.public_value()?.as_bool()?)),
            DataType::Uint(w) => uint(w, chosen.public_value()?.as_uint()?),
        }
    }
}
