#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Private-check workflow data is invalid")]
    Workflow(#[from] check_workflow::error::Error),
    #[error("Invalid arbiter configuration")]
    Configuration,
    #[error("Required setting {0} is missing")]
    MissingSetting(&'static str),
    #[error("GATEWAY_URL requires HTTPS; only explicit LOCAL_DEMO permits loopback HTTP")]
    Transport,
    #[error("Artifact is missing, corrupt or exceeds its limit")]
    Artifact,
    #[error("Private-check proof capability is unavailable")]
    ProofUnavailable,
    #[error("Response does not match the current computation step")]
    Response,
    #[error("Node gateway request failed")]
    Gateway,
    #[error("Node ownership was lost")]
    Ownership,
    #[error("Storage operation failed")]
    Storage(#[from] sqlx::Error),
    #[error("Private-check operation failed")]
    Check(#[from] private_check::CheckError),
    #[error("TFHE operation failed")]
    Math(#[from] tfhe_math::MathError),
}

pub type Result<T> = std::result::Result<T, Error>;
