//! Private-check workflow data shared by the node and development custodian.
//! Private records and local artifacts are not public API projections.
pub mod artifacts;
pub mod messages;

pub mod error {
    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("Artifact is missing, corrupt or exceeds its limit")]
        Artifact,
        #[error("Private-check response has an invalid context")]
        Response,
        #[error("Private-check operation failed")]
        Check(#[from] private_check::CheckError),
        #[error("TFHE operation failed")]
        Math(#[from] tfhe_math::MathError),
    }

    pub type Result<T> = std::result::Result<T, Error>;
}
