use crate::{CheckError, Result};
use rand::{TryRngCore, rngs::OsRng};
use tfhe_math::Width;
use zeroize::Zeroizing;

pub(crate) fn bytes<const N: usize>() -> Result<Zeroizing<[u8; N]>> {
    let mut bytes = Zeroizing::new([0; N]);
    OsRng
        .try_fill_bytes(bytes.as_mut())
        .map_err(|_| CheckError::EntropyUnavailable)?;

    Ok(bytes)
}

pub(crate) fn scalar(width: Width) -> Result<Zeroizing<u64>> {
    let bytes = bytes::<8>()?;
    Ok(Zeroizing::new(u64::from_le_bytes(*bytes) & width.maximum()))
}

pub(crate) fn bounded(width: Width, nonzero: bool) -> Result<Zeroizing<u64>> {
    bounded_with(width, nonzero, |buffer| {
        OsRng
            .try_fill_bytes(buffer)
            .map_err(|_| CheckError::EntropyUnavailable)
    })
}

fn bounded_with(
    width: Width,
    nonzero: bool,
    mut fill: impl FnMut(&mut [u8]) -> Result<()>,
) -> Result<Zeroizing<u64>> {
    for _ in 0..128 {
        let mut bytes = Zeroizing::new([0; 8]);
        fill(bytes.as_mut())?;
        let sample = Zeroizing::new(u64::from_le_bytes(*bytes) & (width.maximum() / 2));

        if !nonzero || *sample != 0 {
            return Ok(sample);
        }
    }

    Err(CheckError::EntropyUnavailable)
}

#[cfg(test)]
#[path = "../tests/unit/mask_sampling.rs"]
mod tests;
