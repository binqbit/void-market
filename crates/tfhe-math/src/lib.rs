//! Typed unsigned/Boolean mathematics with explicit backend and width semantics.
//! Public-reference observations and native ciphertexts are distinct values;
//! neither is a protocol proof or an authorization to disclose a result.

#[cfg(feature = "tfhe-backend")]
mod contexts;
mod errors;
#[cfg(feature = "tfhe-backend")]
mod keys;
mod operations;
mod parameters;
mod session;
#[cfg(feature = "tfhe-backend")]
mod tfhe;
mod types;
mod value;

#[cfg(feature = "tfhe-backend")]
pub mod codec;
#[cfg(feature = "tfhe-backend")]
pub mod secret_codec;
pub mod width_policy;

#[cfg(feature = "tfhe-backend")]
pub use contexts::{DecryptionContext, EncryptionContext, EvaluationContext};

pub use errors::{MathError, Result};
#[cfg(feature = "tfhe-backend")]
pub use keys::{ClientKey, EvaluationKey, KeyBinding, KeySet, PublicKey};

pub use parameters::MathParameters;
pub use session::MathSession;
pub use types::{BackendKind, DataType, Visibility, Width};

pub use value::{FlaggedValue, PublicValue, TfheValue};
