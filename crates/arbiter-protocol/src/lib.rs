//! Metadata shared by the market gateway and independently identified arbiters.
//! Ciphertexts, private keys and owner openings do not belong in these records.
mod authentication;
mod jobs;
mod results;
pub use results::{sign_result, verify_result};

pub use authentication::{RequestSignature, SigningKey, public_key, sign, verify};
pub use jobs::{
    ArtifactRef, Claim, Completion, Job, JobInput, JobStatus, NewJob, NodeProvision, Registration,
};

pub const SCHEMA_VERSION: u8 = 1;
pub const BODY_LIMIT: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Invalid node authentication")]
    Authentication,
}
