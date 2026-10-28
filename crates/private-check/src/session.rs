use crate::{CheckContext, CheckError, ComputationContext, Result};

/// Bound mathematical computation capability, with no private decryption keys.
#[derive(Debug)]
pub struct CheckSession {
    pub(crate) context: CheckContext,
    pub(crate) source: ComputationContext,
    pub(crate) target: ComputationContext,
}

impl CheckSession {
    pub fn new(
        context: CheckContext,
        source: ComputationContext,
        target: ComputationContext,
    ) -> Result<Self> {
        if context.source.key != source.binding() || context.target.key != target.binding() {
            return Err(CheckError::KeyDomainMismatch);
        }

        Ok(Self {
            context,
            source,
            target,
        })
    }

    pub fn context(&self) -> &CheckContext {
        &self.context
    }
}
