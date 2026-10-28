use super::{masks::PreparedBooleanMasks, owner_masks::OwnerMaskContribution};
use crate::{Result, values::CiphertextArtifact};
use tfhe_math::{MathSession, TfheValue, Width};

pub(super) struct ProtectedBoolean {
    pub artifact: CiphertextArtifact,
    pub arithmetic_checks: [CiphertextArtifact; 2],
}

/// Encode B as (B * R1 + R2) XOR S, then hide it under both owners' masks.
/// Overflow flags remain encrypted; producing them does not verify a remote proof.
pub(super) fn protect(
    math: &MathSession,
    width: Width,
    bit: &TfheValue,
    masks: &PreparedBooleanMasks,
    owners: [&OwnerMaskContribution; 2],
) -> Result<ProtectedBoolean> {
    let numeric_bit = math.bool_to_uint(bit, width)?;
    let product = math.mul_in_width(width, &numeric_bit, &masks.encrypted[0].value)?;
    let sum = math.add_in_width(width, &product.value, &masks.encrypted[1].value)?;
    let encoded = math.xor(&sum.value, &masks.encrypted[2].value)?;

    let first_pad = math.xor(&encoded, &owners[0].artifact.value)?;
    let protected = math.xor(&first_pad, &owners[1].artifact.value)?;

    Ok(ProtectedBoolean {
        artifact: CiphertextArtifact::from_value(protected)?,
        arithmetic_checks: [
            CiphertextArtifact::from_value(product.flag)?,
            CiphertextArtifact::from_value(sum.flag)?,
        ],
    })
}
