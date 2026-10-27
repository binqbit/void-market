use crate::{
    MathSession, Result, TfheValue,
    types::{BinaryOp, UnaryOp},
};

impl MathSession {
    pub fn xor(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
        Ok(self.eval_binary(BinaryOp::Xor, lhs, rhs, None)?.value)
    }

    pub fn and(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
        Ok(self.eval_binary(BinaryOp::And, lhs, rhs, None)?.value)
    }

    pub fn or(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
        Ok(self.eval_binary(BinaryOp::Or, lhs, rhs, None)?.value)
    }

    pub fn not(&self, value: &TfheValue) -> Result<TfheValue> {
        self.eval_unary(UnaryOp::Not, value.data_type(), value)
    }
}
