use crate::{DataType, MathError, MathSession, Result, TfheValue};

impl MathSession {
    /// Oblivious selection with zero-extension to the wider branch type.
    /// Any encrypted input makes the result encrypted, including an unselected
    /// branch. Every encrypted input must belong to this session's key domain.
    pub fn select(
        &self,
        condition: &TfheValue,
        yes: &TfheValue,
        no: &TfheValue,
    ) -> Result<TfheValue> {
        if condition.data_type() != DataType::Bool {
            return Err(MathError::TypeMismatch);
        }

        let output = match (yes.data_type(), no.data_type()) {
            (DataType::Bool, DataType::Bool) => DataType::Bool,
            (DataType::Uint(a), DataType::Uint(b)) => DataType::Uint(a.max(b)),
            _ => return Err(MathError::TypeMismatch),
        };

        self.eval_select(output, condition, yes, no)
    }
}
