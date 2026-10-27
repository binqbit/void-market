//! Native TFHE data and operations; SDK values and key-domain policy live above this adapter.
pub(super) mod arithmetic;
pub(super) mod bitwise;
pub(super) mod casts;
pub(crate) mod ciphertext;
pub(super) mod comparison;
pub(crate) mod key_scope;
pub(crate) mod keys;
pub(crate) mod parameters;
pub(super) mod selection;
pub(crate) mod serialization;
pub(crate) mod value;

#[cfg(test)]
#[path = "../../tests/unit/mod.rs"]
mod tests;
