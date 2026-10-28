use crate::checks::commitments;
use crate::values::CiphertextArtifact;
use crate::{CheckError, CheckSession, Digest, Result, random};
use tfhe_math::{PublicValue, Width};
use zeroize::Zeroizing;

pub(crate) struct BooleanMasks {
    pub spacing: Zeroizing<u64>,
    pub offset: Zeroizing<u64>,
    pub pad: Zeroizing<u64>,
}

impl BooleanMasks {
    pub(crate) fn generate(width: Width) -> Result<Self> {
        Ok(Self {
            spacing: random::bounded(width, true)?,
            offset: random::bounded(width, false)?,
            pad: random::scalar(width)?,
        })
    }

    pub(crate) fn endpoints(&self, width: Width) -> Result<(u64, u64)> {
        let limit = width.maximum() / 2;
        if *self.spacing == 0
            || *self.spacing > limit
            || *self.offset > limit
            || *self.pad > width.maximum()
        {
            return Err(CheckError::InvalidMask);
        }

        let upper = self
            .spacing
            .checked_add(*self.offset)
            .ok_or(CheckError::InvalidMask)?;
        if upper > width.maximum() {
            return Err(CheckError::InvalidMask);
        }
        Ok((*self.offset, upper))
    }

    pub(crate) fn decode(&self, width: Width, masked: u64) -> Result<bool> {
        if masked > width.maximum() {
            return Err(CheckError::AnswerOutOfRange);
        }
        let (lower, upper) = self.endpoints(width)?;
        let inner = Zeroizing::new(masked ^ *self.pad);
        if *inner == lower {
            return Ok(false);
        }
        if *inner == upper {
            return Ok(true);
        }
        Err(CheckError::InvalidBooleanAnswer)
    }
}

pub struct PreparedBooleanMasks {
    pub(crate) context: Digest,
    pub(crate) commitment: Digest,
    pub(crate) masks: BooleanMasks,
    pub(crate) encrypted: [CiphertextArtifact; 3],
}

impl PreparedBooleanMasks {
    pub fn context(&self) -> Digest {
        self.context
    }

    pub fn commitment(&self) -> Digest {
        self.commitment
    }
}

impl CheckSession {
    pub fn prepare_boolean_masks(&self) -> Result<PreparedBooleanMasks> {
        let width = self.context.width();
        let masks = BooleanMasks::generate(width)?;
        masks.endpoints(width)?;

        let encryption = &self.target.encryption;
        let encrypted = [
            CiphertextArtifact::encrypt(encryption, PublicValue::uint(width, *masks.spacing)?)?,
            CiphertextArtifact::encrypt(encryption, PublicValue::uint(width, *masks.offset)?)?,
            CiphertextArtifact::encrypt(encryption, PublicValue::uint(width, *masks.pad)?)?,
        ];
        let commitment = commitments::base(
            self.context.id,
            width,
            [
                encrypted[0].wire.native_payload(),
                encrypted[1].wire.native_payload(),
                encrypted[2].wire.native_payload(),
            ],
        )?;

        Ok(PreparedBooleanMasks {
            context: self.context.id,
            commitment,
            masks,
            encrypted,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/boolean_endpoints.rs"]
mod tests;
