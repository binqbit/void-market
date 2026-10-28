use super::masks::NumericMasks;
use super::{PreparedTransfer, TransferBinding, TransferChallenge};
use crate::checks::commitments;
use crate::values::CiphertextArtifact;
use crate::{CheckError, CheckSession, NumberInput, Result};
use tfhe_math::PublicValue;

impl CheckSession {
    pub fn prepare_number_transfer(&self, input: &NumberInput) -> Result<PreparedTransfer> {
        if input.binding != self.context.source {
            return Err(CheckError::ReferenceMismatch);
        }
        let width = self.context.width();
        let masks = NumericMasks::generate(width)?;

        let offset = self
            .source
            .encryption
            .encrypt(PublicValue::uint(width, *masks.offset)?)?;
        let pad = self
            .source
            .encryption
            .encrypt(PublicValue::uint(width, *masks.pad)?)?;
        let math = self.source.math();
        let sum = math.add_in_width(width, &input.value, &offset)?;
        let masked = math.xor(&sum.value, &pad)?;

        let artifact = CiphertextArtifact::from_value(masked)?;
        let binding = TransferBinding {
            context: self.context.id,
            reference: input.binding.native,
        };
        let id = commitments::transfer_reference(
            binding.context,
            binding.reference,
            artifact.wire.digest(),
        )?;

        Ok(PreparedTransfer {
            challenge: TransferChallenge {
                binding,
                artifact,
                id,
            },
            masks,
            range: self.check_number(input)?,
            overflow: CiphertextArtifact::from_value(sum.flag)?,
        })
    }
}
