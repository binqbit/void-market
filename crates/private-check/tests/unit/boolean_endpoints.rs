use super::BooleanMasks;
use crate::CheckError;
use tfhe_math::Width;
use zeroize::Zeroizing;

#[test]
fn every_u8_spacing_offset_and_bit_decodes_exactly() {
    for spacing in 1..=127 {
        for offset in 0..=127 {
            for pad in [0, 255] {
                let masks = BooleanMasks {
                    spacing: Zeroizing::new(spacing),
                    offset: Zeroizing::new(offset),
                    pad: Zeroizing::new(pad),
                };
                for bit in [false, true] {
                    let encoded = (u64::from(bit) * spacing + offset) ^ pad;
                    assert_eq!(masks.decode(Width::U8, encoded).unwrap(), bit);
                    assert_eq!(
                        masks.decode(Width::U8, encoded ^ 128),
                        Err(CheckError::InvalidBooleanAnswer)
                    );
                }
            }
        }
    }
}

#[test]
fn zero_spacing_and_out_of_profile_masks_are_invalid() {
    for (spacing, offset, pad) in [(0, 0, 0), (128, 0, 0), (1, 128, 0), (1, 0, 256)] {
        let masks = BooleanMasks {
            spacing: Zeroizing::new(spacing),
            offset: Zeroizing::new(offset),
            pad: Zeroizing::new(pad),
        };
        assert_eq!(masks.endpoints(Width::U8), Err(CheckError::InvalidMask));
    }
}
