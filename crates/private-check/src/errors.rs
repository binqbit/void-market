use tfhe_math::MathError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error(transparent)]
    Math(#[from] MathError),
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
    #[error("invalid owner identity or repeated owner")]
    InvalidOwner,
    #[error("number exceeds the selected private input range")]
    NumberOutOfRange,
    #[error("value has the wrong declared type or width")]
    TypeMismatch,
    #[error("key material differs from the expected domain")]
    KeyDomainMismatch,
    #[error("source and target must be independent key domains")]
    SameKeyDomain,
    #[error("operation context does not match")]
    ContextMismatch,
    #[error("original input or challenge reference does not match")]
    ReferenceMismatch,
    #[error("commitment does not match its original opening")]
    CommitmentMismatch,
    #[error("invalid mask range or Boolean endpoints")]
    InvalidMask,
    #[error("protected answer is outside its declared width")]
    AnswerOutOfRange,
    #[error("protected answer matches neither Boolean endpoint")]
    InvalidBooleanAnswer,
    #[error("canonical encoding failed")]
    Encoding,
}

pub type Result<T> = std::result::Result<T, CheckError>;
