use crate::{DataType, MathError, Result, Width, types::BinaryOp};

pub fn addition_width(a: Width, b: Width) -> Result<Width> {
    Width::containing(u16::from(a.max(b).bits()) + 1)
}

pub fn multiplication_width(a: Width, b: Width) -> Result<Width> {
    Width::containing(u16::from(a.bits()) + u16::from(b.bits()))
}

pub fn widening(from: Width, to: Width) -> Result<()> {
    if to < from {
        Err(MathError::NarrowingForbidden {
            from: from.bits(),
            to: to.bits(),
        })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BinaryPlan {
    pub op: BinaryOp,
    pub operands: DataType,
    pub output: DataType,
    pub flagged: bool,
}

pub(crate) fn plan_binary(
    op: BinaryOp,
    a: DataType,
    b: DataType,
    target: Option<Width>,
) -> Result<BinaryPlan> {
    match (a, b) {
        (DataType::Uint(a), DataType::Uint(b)) => {
            let width = match op {
                BinaryOp::AddWide => addition_width(a, b)?,
                BinaryOp::MulWide => multiplication_width(a, b)?,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                    let w = target.ok_or(MathError::InvalidPlan)?;
                    widening(a, w)?;
                    widening(b, w)?;

                    w
                }
                _ => a.max(b),
            };

            let output = if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
            ) {
                DataType::Bool
            } else {
                DataType::Uint(width)
            };

            Ok(BinaryPlan {
                op,
                operands: DataType::Uint(width),
                output,
                flagged: matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul),
            })
        }
        (DataType::Bool, DataType::Bool)
            if matches!(
                op,
                BinaryOp::Xor | BinaryOp::And | BinaryOp::Or | BinaryOp::Eq | BinaryOp::Ne
            ) =>
        {
            Ok(BinaryPlan {
                op,
                operands: DataType::Bool,
                output: DataType::Bool,
                flagged: false,
            })
        }
        _ => Err(MathError::TypeMismatch),
    }
}
