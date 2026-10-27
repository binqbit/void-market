//! SDK value representations, separate from native TFHE execution.
mod public;
mod tfhe_value;

pub use public::PublicValue;
pub use tfhe_value::{FlaggedValue, TfheValue};

pub(crate) use public::Literal;
#[cfg(feature = "tfhe-backend")]
pub(crate) use tfhe_value::Storage;
