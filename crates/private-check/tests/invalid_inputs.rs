use super::support::{Fixture, owners, source_keys, target_keys};
use private_check::{
    CheckContext, CheckError, Comparison, ComputationContext, KeyOwnerContext, NumberInput, OwnerId,
};
use tfhe_math::{
    PublicValue, Width,
    codec::{EncodingLimits, encrypt_compact},
};

#[test]
fn public_ranges_identities_and_domains_are_checked() {
    assert!(matches!(
        NumberInput::encrypt(&source_keys().encryption_context(), Width::U8, 128),
        Err(CheckError::NumberOutOfRange)
    ));
    assert!(matches!(
        OwnerId::new([0; 16]),
        Err(CheckError::InvalidOwner)
    ));
    assert!(matches!(
        ComputationContext::new(
            source_keys().encryption_context(),
            target_keys().evaluation_context()
        ),
        Err(CheckError::KeyDomainMismatch)
    ));
    assert!(matches!(
        KeyOwnerContext::new(
            source_keys().decryption_context(),
            target_keys().evaluation_context()
        ),
        Err(CheckError::KeyDomainMismatch)
    ));

    let fixture = Fixture::new(Width::U8, 5, 6, Comparison::Less);
    assert!(matches!(
        CheckContext::new(
            [1; 32],
            [owners()[0]; 2],
            Comparison::Equal,
            &fixture.source,
            &fixture.target
        ),
        Err(CheckError::InvalidOwner)
    ));
    assert!(matches!(
        CheckContext::new(
            [1; 32],
            owners(),
            Comparison::Equal,
            &fixture.source,
            &fixture.source
        ),
        Err(CheckError::SameKeyDomain)
    ));
    let wider = NumberInput::encrypt(&target_keys().encryption_context(), Width::U16, 6).unwrap();
    assert!(matches!(
        CheckContext::new(
            [1; 32],
            owners(),
            Comparison::Equal,
            &fixture.source,
            &wider
        ),
        Err(CheckError::TypeMismatch)
    ));
}

#[test]
fn compact_import_retains_distinct_original_frame_and_payload_identities() {
    let fixture = Fixture::new(Width::U8, 7, 9, Comparison::Less);
    let wire = fixture.source.encoded();
    let imported = NumberInput::import(
        wire.as_bytes(),
        wire.digest(),
        Width::U8,
        &source_keys().encryption_context(),
    )
    .unwrap();
    assert_eq!(imported.binding(), fixture.source.binding());
    assert_eq!(imported.encoded().as_bytes(), wire.as_bytes());
    assert_ne!(
        imported.binding().frame_digest(),
        imported.binding().native_digest()
    );
    assert!(
        NumberInput::import(
            wire.as_bytes(),
            [0; 32],
            Width::U8,
            &source_keys().encryption_context()
        )
        .is_err()
    );
    assert!(
        NumberInput::import(
            wire.as_bytes(),
            wire.digest(),
            Width::U16,
            &source_keys().encryption_context()
        )
        .is_err()
    );

    // An untrusted native import has no proved range; the fixed predicate stays encrypted.
    let out_of_range = encrypt_compact(
        &source_keys().encryption_context(),
        PublicValue::uint(Width::U8, 255).unwrap(),
        EncodingLimits::default(),
    )
    .unwrap();
    let input = NumberInput::import(
        out_of_range.as_bytes(),
        out_of_range.digest(),
        Width::U8,
        &source_keys().encryption_context(),
    )
    .unwrap();
    let context = CheckContext::new(
        [3; 32],
        owners(),
        Comparison::Greater,
        &input,
        &fixture.target,
    )
    .unwrap();
    let session = private_check::CheckSession::new(
        context,
        ComputationContext::new(
            source_keys().encryption_context(),
            source_keys().evaluation_context(),
        )
        .unwrap(),
        ComputationContext::new(
            target_keys().encryption_context(),
            target_keys().evaluation_context(),
        )
        .unwrap(),
    )
    .unwrap();
    let check = session.check_number(&input).unwrap();
    assert!(!super::support::flag(check.encoded(), source_keys()));
}
