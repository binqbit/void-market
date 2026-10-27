mod arithmetic;
mod bitwise;
mod casts;
mod comparison;
mod selection;

#[cfg(feature = "tfhe-backend")]
mod encrypted;
mod executor;
pub(crate) mod public_reference;

pub(crate) use executor::{BinaryResult, OperationExecutor};
