use super::support::{encrypted, foreign_keys, keys, session, uint};
use tfhe::prelude::FheTrivialEncrypt;
use tfhe_math::{KeyBinding, MathError, MathParameters, PublicValue, Width};

#[test]
fn namespace_fingerprints_are_pinned() {
    let config = MathParameters.fingerprint().unwrap();
    let expected_config = [
        0xfa, 0x7b, 0xc1, 0x1e, 0x88, 0x0b, 0xd7, 0x69, 0x9f, 0x9d, 0xc4, 0x75, 0xf1, 0x55, 0x32,
        0x7f, 0x3b, 0x82, 0x1c, 0x23, 0x73, 0x95, 0xc1, 0xbe, 0x08, 0x7e, 0x2b, 0x23, 0xf4, 0x11,
        0x4c, 0x03,
    ];
    assert_eq!(config, expected_config);

    let binding = KeyBinding::from_fingerprints(config, [1; 32], [2; 32]).unwrap();
    let expected_domain = [
        0x6e, 0xeb, 0x0e, 0x45, 0x24, 0xe3, 0xa0, 0x49, 0x25, 0x30, 0x10, 0xc3, 0xbd, 0x80, 0xe9,
        0xa3, 0x25, 0xed, 0xd1, 0xee, 0x64, 0x1f, 0xf1, 0x79, 0xde, 0xf6, 0x28, 0x28, 0x36, 0x04,
        0x7b, 0xbd,
    ];
    assert_eq!(binding.domain_id(), expected_domain);
}

#[test]
fn different_domains_are_rejected_even_on_unselected_branches() {
    let other = foreign_keys();
    let alien = other
        .encryption_context()
        .encrypt(PublicValue::uint(Width::U8, 3).unwrap())
        .unwrap();
    let local = encrypted(Width::U8, 3);
    let s = session();
    assert_ne!(keys().binding(), other.binding());
    assert!(matches!(
        s.eq(&local, &alien),
        Err(MathError::KeyDomainMismatch)
    ));
    assert!(matches!(
        s.select(&PublicValue::boolean(true).into(), &local, &alien),
        Err(MathError::KeyDomainMismatch)
    ));
    assert!(matches!(
        keys().decryption_context().decrypt(&alien),
        Err(MathError::KeyDomainMismatch)
    ));
    assert!(matches!(
        KeyBinding::from_fingerprints([0; 32], [1; 32], [2; 32]),
        Err(MathError::ConfigMismatch)
    ));
}

#[test]
fn thread_local_key_is_cleared_and_contexts_can_move_between_threads() {
    let value = encrypted(Width::U8, 17);
    let jobs: Vec<_> = (0..2)
        .map(|_| {
            let v = value.clone();
            std::thread::spawn(move || {
                assert_eq!(uint(&session().not(&v).unwrap()), 238);
                // Upstream encrypt_trivial needs an installed server key. A successful SDK
                // operation must not leave one behind on its caller's OS thread.
                assert!(std::panic::catch_unwind(|| tfhe::FheUint8::encrypt_trivial(0u8)).is_err());
            })
        })
        .collect();
    for job in jobs {
        job.join().unwrap()
    }
}
