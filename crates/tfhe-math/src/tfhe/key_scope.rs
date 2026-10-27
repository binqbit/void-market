//! Synchronous scopes for TFHE's OS-thread-local server key.
use crate::{MathError, Result};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct ClearServerKeyOnDrop;

impl Drop for ClearServerKeyOnDrop {
    fn drop(&mut self) {
        ::tfhe::unset_server_key();
    }
}

/// Only synchronous internal operations enter this scope; no public Future-returning closure.
pub(crate) fn with_server_key<T>(
    key: &::tfhe::ServerKey,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ::tfhe::unset_server_key();
    let _clear = ClearServerKeyOnDrop;

    catch_unwind(AssertUnwindSafe(|| {
        ::tfhe::with_server_key_as_context(key.clone(), work)
    }))
    .map_err(|_| MathError::BackendFailure)?
}

pub(crate) fn without_server_key<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    ::tfhe::unset_server_key();
    let _clear = ClearServerKeyOnDrop;

    catch_unwind(AssertUnwindSafe(work)).map_err(|_| MathError::BackendFailure)?
}
