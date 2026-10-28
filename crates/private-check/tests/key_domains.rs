use super::support::{Fixture, source_keys, source_owner, target_keys, target_owner};
use private_check::{
    CheckContext, CheckError, Comparison, InputBinding, TransferBinding, TransferChallenge,
    TransferResponse,
};
use tfhe_math::Width;

#[test]
fn public_context_roundtrip_preserves_the_original_operation_without_secret_state() {
    let fixture = Fixture::new(Width::U8, 19, 20, Comparison::Less);
    let original = fixture.context();
    let copy_binding = |binding: InputBinding| {
        InputBinding::from_parts(
            binding.frame_digest(),
            binding.native_digest(),
            binding.key_binding(),
            binding.width(),
        )
    };
    let restored = CheckContext::from_parts(
        original.external_context(),
        original.nonce(),
        original.owners(),
        original.comparison(),
        copy_binding(original.source()),
        copy_binding(original.target()),
    )
    .unwrap();
    assert_eq!(restored.id(), original.id());

    let mut wrong_nonce = original.nonce();
    wrong_nonce[0] ^= 1;
    let changed = CheckContext::from_parts(
        original.external_context(),
        wrong_nonce,
        original.owners(),
        original.comparison(),
        original.source(),
        original.target(),
    )
    .unwrap();
    assert_ne!(changed.id(), original.id());
}

#[test]
fn transfer_rejects_replays_wrong_keys_and_substituted_references() {
    let fixture = Fixture::new(Width::U8, 19, 20, Comparison::Less);
    let prepared = fixture
        .session
        .prepare_number_transfer(&fixture.source)
        .unwrap();
    let challenge = prepared.challenge();
    let context = fixture.context();
    let other = CheckContext::new(
        [9; 32],
        context.owners(),
        context.comparison(),
        &fixture.source,
        &fixture.target,
    )
    .unwrap();
    assert_ne!(context.id(), other.id());

    assert!(matches!(
        source_owner().reencrypt_masked(challenge, &target_keys().encryption_context(), &other),
        Err(CheckError::ContextMismatch)
    ));
    assert!(matches!(
        target_owner().reencrypt_masked(challenge, &target_keys().encryption_context(), context),
        Err(CheckError::KeyDomainMismatch)
    ));
    assert!(matches!(
        source_owner().reencrypt_masked(challenge, &source_keys().encryption_context(), context),
        Err(CheckError::KeyDomainMismatch)
    ));

    let bad = TransferBinding {
        reference: [0; 32],
        ..challenge.binding()
    };
    assert!(matches!(
        TransferChallenge::import(
            bad,
            challenge.encoded().as_bytes(),
            challenge.encoded().digest(),
            context,
            &source_keys().evaluation_context()
        ),
        Err(CheckError::ReferenceMismatch)
    ));
    let mut corrupted = challenge.encoded().as_bytes().to_vec();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(
        TransferChallenge::import(
            challenge.binding(),
            &corrupted,
            challenge.encoded().digest(),
            context,
            &source_keys().evaluation_context()
        )
        .is_err()
    );

    let response = source_owner()
        .reencrypt_masked(challenge, &target_keys().encryption_context(), context)
        .unwrap();
    let wrong = TransferBinding {
        reference: [0; 32],
        ..response.binding()
    };
    assert!(matches!(
        TransferResponse::import(
            wrong,
            response.encoded().as_bytes(),
            response.encoded().digest(),
            challenge,
            context,
            &target_keys().evaluation_context()
        ),
        Err(CheckError::ReferenceMismatch)
    ));
    assert!(matches!(
        TransferResponse::import(
            response.binding(),
            response.encoded().as_bytes(),
            response.encoded().digest(),
            challenge,
            context,
            &source_keys().evaluation_context()
        ),
        Err(CheckError::KeyDomainMismatch)
    ));
}

#[test]
fn another_pending_transfer_cannot_consume_the_response() {
    let fixture = Fixture::new(Width::U8, 19, 20, Comparison::Less);
    let first = fixture
        .session
        .prepare_number_transfer(&fixture.source)
        .unwrap();
    let second = fixture
        .session
        .prepare_number_transfer(&fixture.source)
        .unwrap();
    assert_ne!(first.challenge().id(), second.challenge().id());
    let response = source_owner()
        .reencrypt_masked(
            second.challenge(),
            &target_keys().encryption_context(),
            fixture.context(),
        )
        .unwrap();
    assert!(matches!(
        fixture.session.finish_number_transfer(first, response),
        Err(CheckError::ReferenceMismatch)
    ));
}
