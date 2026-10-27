use crate::tfhe::key_scope::{with_server_key, without_server_key};
use crate::tfhe::parameters;
use crate::{MathError, Result};
use ::tfhe::prelude::{FheDecrypt, FheTrivialEncrypt};
use std::panic::catch_unwind;

#[test]
fn error_and_unwind_both_clear_the_installed_key() {
    let config = parameters::config();
    let client = ::tfhe::ClientKey::generate(config);
    let server = client.generate_server_key();

    // TFHE deliberately panics without a server key. With --nocapture, Rust prints
    // these expected panics even though catch_unwind handles them successfully.
    let check_empty = || {
        assert!(catch_unwind(|| ::tfhe::FheUint8::encrypt_trivial(0u8)).is_err());
    };

    let failure: Result<()> = with_server_key(&server, || Err(MathError::InvalidEncoding));
    assert_eq!(failure, Err(MathError::InvalidEncoding));
    check_empty();

    let failure: Result<()> = with_server_key(&server, || panic!("injected native unwind"));
    assert_eq!(failure, Err(MathError::BackendFailure));
    check_empty();

    // Neither a pre-existing foreign key nor the panic may escape the scope.
    ::tfhe::set_server_key(server.clone());
    let failure: Result<()> = without_server_key(|| panic!("injected parse unwind"));
    assert_eq!(failure, Err(MathError::BackendFailure));
    check_empty();

    with_server_key(&server, || {
        let value = ::tfhe::FheUint8::encrypt_trivial(7u8);
        let actual: u8 = value.decrypt(&client);
        assert_eq!(actual, 7);
        Ok(())
    })
    .unwrap();
    check_empty();
}
