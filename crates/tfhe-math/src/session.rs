use crate::operations::{BinaryResult, OperationExecutor, public_reference::PublicReference};
use crate::{
    BackendKind, DataType, MathError, Result, TfheValue, Visibility, Width,
    types::{BinaryOp, UnaryOp},
};

enum SessionExecution {
    Reference(PublicReference),
    #[cfg(feature = "tfhe-backend")]
    Encrypted(crate::contexts::EvaluationContext),
}

/// A session fixes its execution mode. Encrypted sessions bind one key domain.
/// Evaluation never falls back to plaintext computation.
pub struct MathSession {
    execution: SessionExecution,
}

impl MathSession {
    /// Public literals only; reference observations carry no protocol proof.
    pub const fn public_reference() -> Self {
        Self {
            execution: SessionExecution::Reference(PublicReference),
        }
    }

    /// Bind evaluation to exactly one key domain. Each operation requires at
    /// least one encrypted input; public constants are lifted inside the scope.
    #[cfg(feature = "tfhe-backend")]
    pub fn encrypted(context: crate::EvaluationContext) -> Self {
        Self {
            execution: SessionExecution::Encrypted(context),
        }
    }

    pub fn backend_kind(&self) -> BackendKind {
        match self.execution {
            SessionExecution::Reference(_) => BackendKind::PublicReference,
            #[cfg(feature = "tfhe-backend")]
            SessionExecution::Encrypted(_) => BackendKind::EncryptedTfhe,
        }
    }

    pub(crate) fn eval_binary(
        &self,
        operation: BinaryOp,
        lhs: &TfheValue,
        rhs: &TfheValue,
        target: Option<Width>,
    ) -> Result<BinaryResult> {
        let plan =
            crate::width_policy::plan_binary(operation, lhs.data_type(), rhs.data_type(), target)?;
        self.validate_inputs(&[lhs, rhs])?;

        self.executor().binary(plan, lhs, rhs)
    }

    pub(crate) fn eval_unary(
        &self,
        operation: UnaryOp,
        output: DataType,
        value: &TfheValue,
    ) -> Result<TfheValue> {
        self.validate_inputs(&[value])?;

        self.executor().unary(operation, output, value)
    }

    pub(crate) fn eval_select(
        &self,
        output: DataType,
        condition: &TfheValue,
        yes: &TfheValue,
        no: &TfheValue,
    ) -> Result<TfheValue> {
        self.validate_inputs(&[condition, yes, no])?;

        self.executor().select(output, condition, yes, no)
    }

    fn validate_inputs(&self, values: &[&TfheValue]) -> Result<()> {
        if self.backend_kind() == BackendKind::PublicReference
            && values
                .iter()
                .any(|value| value.visibility() == Visibility::Encrypted)
        {
            Err(MathError::BackendMismatch)
        } else {
            Ok(())
        }
    }

    fn executor(&self) -> &dyn OperationExecutor {
        match &self.execution {
            SessionExecution::Reference(executor) => executor,
            #[cfg(feature = "tfhe-backend")]
            SessionExecution::Encrypted(executor) => executor,
        }
    }
}
