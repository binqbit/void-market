/// Errors describe public shape, configuration or resource conditions. Secret
/// arithmetic overflow is returned as a value, never as one of these errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MathError {
    #[error("operation is not defined for these value types")]
    TypeMismatch,
    #[error("no supported result width for {required_bits} bits")]
    UnsupportedWidth { required_bits: u16 },
    #[error("narrowing from {from} to {to} bits is forbidden")]
    NarrowingForbidden { from: u8, to: u8 },
    #[error("literal does not fit its declared width")]
    LiteralOutOfRange,
    #[error("value does not belong to this backend")]
    BackendMismatch,
    #[error("encrypted operands belong to different key domains")]
    KeyDomainMismatch,
    #[error("configuration differs from the expected configuration")]
    ConfigMismatch,
    #[error("invalid or nonconformant encoding")]
    InvalidEncoding,
    #[error("encoded object exceeds its resource limit")]
    ResourceLimit,
    #[error("native backend failed")]
    BackendFailure,
    #[error("internal operation-plan invariant failed")]
    InvalidPlan,
}

pub type Result<T> = std::result::Result<T, MathError>;
