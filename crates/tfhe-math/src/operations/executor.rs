use crate::{DataType, Result, TfheValue, types::UnaryOp, width_policy::BinaryPlan};

pub(crate) struct BinaryResult {
    pub value: TfheValue,
    pub flag: Option<TfheValue>,
}

pub(crate) trait OperationExecutor {
    fn binary(&self, plan: BinaryPlan, lhs: &TfheValue, rhs: &TfheValue) -> Result<BinaryResult>;
    fn unary(&self, op: UnaryOp, output: DataType, value: &TfheValue) -> Result<TfheValue>;
    fn select(
        &self,
        output: DataType,
        condition: &TfheValue,
        yes: &TfheValue,
        no: &TfheValue,
    ) -> Result<TfheValue>;
}
