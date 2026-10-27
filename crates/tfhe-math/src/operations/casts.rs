use crate::{DataType, MathError, MathSession, Result, TfheValue, Width, types::UnaryOp};

impl MathSession {
    pub fn widen(&self, value: &TfheValue, to: Width) -> Result<TfheValue> {
        let DataType::Uint(from) = value.data_type() else {
            return Err(MathError::TypeMismatch);
        };

        crate::width_policy::widening(from, to)?;
        self.eval_unary(UnaryOp::Widen, DataType::Uint(to), value)
    }

    pub fn bool_to_uint(&self, value: &TfheValue, to: Width) -> Result<TfheValue> {
        if value.data_type() != DataType::Bool {
            return Err(MathError::TypeMismatch);
        }

        self.eval_unary(UnaryOp::BoolToUint, DataType::Uint(to), value)
    }

    pub fn is_nonzero(&self, value: &TfheValue) -> Result<TfheValue> {
        if !matches!(value.data_type(), DataType::Uint(_)) {
            return Err(MathError::TypeMismatch);
        }

        self.eval_unary(UnaryOp::IsNonzero, DataType::Bool, value)
    }
}
