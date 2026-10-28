use super::{PreparedTransfer, TransferResponse, TransferredNumber};
use crate::values::CiphertextArtifact;
use crate::{CheckError, CheckSession, Result};
use tfhe_math::PublicValue;

impl CheckSession {
    /// Consumes ephemeral witnesses. A failed consumed attempt cannot be resumed.
    pub fn finish_number_transfer(
        &self,
        prepared: PreparedTransfer,
        response: TransferResponse,
    ) -> Result<TransferredNumber> {
        self.context.check_id(prepared.challenge.binding.context)?;
        self.context.check_id(response.binding.context)?;
        if prepared.challenge.binding.reference != self.context.source.native
            || response.binding.reference != prepared.challenge.id
        {
            return Err(CheckError::ReferenceMismatch);
        }

        let width = self.context.width();
        let pad = self
            .target
            .encryption
            .encrypt(PublicValue::uint(width, *prepared.masks.pad)?)?;
        let offset = self
            .target
            .encryption
            .encrypt(PublicValue::uint(width, *prepared.masks.offset)?)?;
        let math = self.target.math();
        let unpadded = math.xor(&response.artifact.value, &pad)?;
        let original = math.sub_in_width(width, &unpadded, &offset)?;

        Ok(TransferredNumber {
            context: self.context.id,
            source: self.context.source,
            value: original.value,
            borrow: CiphertextArtifact::from_value(original.flag)?,
        })
    }
}
