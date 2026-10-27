use crate::tfhe::{self as native, value::NativeValue};
use crate::{
    DataType, EvaluationContext, MathError, Result, TfheValue, Visibility,
    operations::{BinaryResult, OperationExecutor},
    types::{BinaryOp, UnaryOp},
    width_policy::BinaryPlan,
};

impl EvaluationContext {
    fn validate_inputs(&self, values: &[&TfheValue]) -> Result<()> {
        let mut encrypted = false;
        for v in values {
            if v.visibility() == Visibility::Encrypted {
                v.encrypted_native(&self.binding())?;
                encrypted = true;
            }
        }
        if !encrypted {
            return Err(MathError::BackendMismatch);
        }

        Ok(())
    }
}

impl OperationExecutor for EvaluationContext {
    fn binary(&self, plan: BinaryPlan, a: &TfheValue, b: &TfheValue) -> Result<BinaryResult> {
        self.validate_inputs(&[a, b])?;

        native::key_scope::with_server_key(&self.key.native, || {
            let a = materialize(a, plan.operands, &self.binding())?;
            let b = materialize(b, plan.operands, &self.binding())?;

            let (result, flag) = match plan.op {
                BinaryOp::AddWide
                | BinaryOp::MulWide
                | BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul => native::arithmetic::evaluate(plan.op, &a, &b)?,
                BinaryOp::Xor | BinaryOp::And | BinaryOp::Or => {
                    (native::bitwise::binary(plan.op, &a, &b)?, None)
                }
                _ => (native::comparison::evaluate(plan.op, &a, &b)?, None),
            };

            if result.data_type() != plan.output {
                return Err(MathError::InvalidPlan);
            }

            Ok(BinaryResult {
                value: TfheValue::from_encrypted(result, self.binding()),
                flag: flag.map(|f| TfheValue::from_encrypted(f, self.binding())),
            })
        })
    }

    fn unary(&self, op: UnaryOp, output: DataType, v: &TfheValue) -> Result<TfheValue> {
        self.validate_inputs(&[v])?;

        native::key_scope::with_server_key(&self.key.native, || {
            let v = v.encrypted_native(&self.binding())?;

            let result = match op {
                UnaryOp::Not => native::bitwise::not(v),
                UnaryOp::Widen | UnaryOp::BoolToUint => native::casts::cast(v, output)?,
                UnaryOp::IsNonzero => native::casts::is_nonzero(v)?,
            };

            Ok(TfheValue::from_encrypted(result, self.binding()))
        })
    }

    fn select(
        &self,
        out: DataType,
        c: &TfheValue,
        a: &TfheValue,
        b: &TfheValue,
    ) -> Result<TfheValue> {
        self.validate_inputs(&[c, a, b])?;

        native::key_scope::with_server_key(&self.key.native, || {
            let c = materialize(c, DataType::Bool, &self.binding())?;
            let a = materialize(a, out, &self.binding())?;
            let b = materialize(b, out, &self.binding())?;

            Ok(TfheValue::from_encrypted(
                native::selection::evaluate(&c, &a, &b)?,
                self.binding(),
            ))
        })
    }
}

fn materialize(
    value: &TfheValue,
    output: DataType,
    binding: &crate::KeyBinding,
) -> Result<NativeValue> {
    if let crate::value::Storage::Public(public) = value.storage {
        let compatible = matches!(
            (public.data_type(), output),
            (DataType::Bool, DataType::Bool) | (DataType::Uint(_), DataType::Uint(_))
        );
        if !compatible {
            return Err(MathError::InvalidPlan);
        }

        let native = native::casts::trivial(public.to_native()?);

        return native::casts::cast(&native, output);
    }

    native::casts::cast(value.encrypted_native(binding)?, output)
}
