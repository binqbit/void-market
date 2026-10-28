//! Mathematical mechanics for masked HE transfer and private comparisons.
//!
//! Context, commitment and endpoint checks are distinct from proofs of honest
//! remote encryption, evaluation or decryption. The host supplies authorization,
//! protected delivery and the required proof verification before disclosure.
mod bindings;
mod checks;
mod comparison;
mod contexts;
mod errors;
mod random;
mod session;
mod transfer;
mod values;

pub use bindings::{CheckContext, Comparison, Digest, InputBinding, OwnerId};
pub use checks::number::{EncryptedNumberCheck, validate_public_number};
pub use comparison::answer::{AnswerCommitment, MaskedBooleanAnswer, PendingBooleanAnswer};
pub use comparison::audit::BooleanAuditOpening;
pub use comparison::evaluate::{
    BooleanChallenge, BooleanChallengeBinding, DecodedBoolean, PreparedComparison,
};
pub use comparison::masks::PreparedBooleanMasks;
pub use comparison::owner_masks::{
    OwnerContributionBinding, OwnerMaskContribution, OwnerMaskOpening, PreparedOwnerMask,
};
pub use contexts::{ComputationContext, KeyOwnerContext};
pub use errors::{CheckError, Result};
pub use session::CheckSession;
pub use transfer::{
    PreparedTransfer, TransferBinding, TransferChallenge, TransferResponse, TransferredNumber,
};
pub use values::NumberInput;

#[cfg(test)]
#[path = "../tests/unit/boolean_audit.rs"]
mod boolean_audit_tests;

#[cfg(test)]
#[path = "../tests/unit/numeric_transfer.rs"]
mod numeric_transfer_tests;
