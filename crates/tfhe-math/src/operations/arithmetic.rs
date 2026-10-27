use crate::{FlaggedValue, MathError, MathSession, Result, TfheValue, Width, types::BinaryOp};

impl MathSession {
    /// Zero-extend both operands before addition. The result uses the smallest
    /// supported width containing `max(lhs.bits, rhs.bits) + 1`; >64 bits is rejected.
    pub fn add_wide(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
        Ok(self.eval_binary(BinaryOp::AddWide, lhs, rhs, None)?.value)
    }

    /// Zero-extend before multiplication to the smallest supported width
    /// containing `lhs.bits + rhs.bits`. No implicit truncation is performed.
    pub fn mul_wide(&self, lhs: &TfheValue, rhs: &TfheValue) -> Result<TfheValue> {
        Ok(self.eval_binary(BinaryOp::MulWide, lhs, rhs, None)?.value)
    }

    /// Return `(lhs + rhs) mod 2^width` and a Boolean overflow value.
    /// The width cannot narrow either operand. An encrypted flag never becomes
    /// a host-side error.
    pub fn add_in_width(
        &self,
        width: Width,
        lhs: &TfheValue,
        rhs: &TfheValue,
    ) -> Result<FlaggedValue> {
        self.flagged(BinaryOp::Add, width, lhs, rhs)
    }

    /// Return `(lhs - rhs) mod 2^width` and the Boolean borrow/underflow value `lhs < rhs`.
    pub fn sub_in_width(
        &self,
        width: Width,
        lhs: &TfheValue,
        rhs: &TfheValue,
    ) -> Result<FlaggedValue> {
        self.flagged(BinaryOp::Sub, width, lhs, rhs)
    }

    /// Return `(lhs * rhs) mod 2^width` and a Boolean overflow value.
    /// The width cannot narrow either operand.
    pub fn mul_in_width(
        &self,
        width: Width,
        lhs: &TfheValue,
        rhs: &TfheValue,
    ) -> Result<FlaggedValue> {
        self.flagged(BinaryOp::Mul, width, lhs, rhs)
    }

    fn flagged(
        &self,
        operation: BinaryOp,
        width: Width,
        lhs: &TfheValue,
        rhs: &TfheValue,
    ) -> Result<FlaggedValue> {
        let evaluation = self.eval_binary(operation, lhs, rhs, Some(width))?;
        Ok(FlaggedValue {
            value: evaluation.value,
            flag: evaluation.flag.ok_or(MathError::InvalidPlan)?,
        })
    }
}
