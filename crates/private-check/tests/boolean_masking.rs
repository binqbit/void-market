use super::support::{Fixture, owners, source_owner, target_keys, target_owner};
use minicbor::Encoder;
use private_check::{CheckError, Comparison, MaskedBooleanAnswer, PreparedOwnerMask};
use sha2::{Digest, Sha256};
use tfhe_math::Width;

#[test]
fn independently_bound_bases_and_duplicate_owners_are_rejected_before_evaluation() {
    let fixture = Fixture::new(Width::U8, 12, 9, Comparison::Greater);
    let transferred = fixture.transfer();
    let first_base = fixture.session.prepare_boolean_masks().unwrap();
    let second_base = fixture.session.prepare_boolean_masks().unwrap();
    let first = PreparedOwnerMask::generate(
        owners()[0],
        fixture.context(),
        first_base.commitment(),
        &target_keys().encryption_context(),
    )
    .unwrap();
    let second = PreparedOwnerMask::generate(
        owners()[1],
        fixture.context(),
        first_base.commitment(),
        &target_keys().encryption_context(),
    )
    .unwrap();
    assert!(matches!(
        fixture.session.compare_protected(
            &transferred,
            &fixture.target,
            second_base,
            first.contribution(),
            second.contribution()
        ),
        Err(CheckError::CommitmentMismatch)
    ));
    assert!(matches!(
        fixture.session.compare_protected(
            &transferred,
            &fixture.target,
            first_base,
            first.contribution(),
            first.contribution()
        ),
        Err(CheckError::InvalidOwner)
    ));
}

#[test]
fn invalid_endpoint_is_rejected_even_with_a_consistent_answer_hash() {
    let fixture = Fixture::new(Width::U8, 12, 9, Comparison::Greater);
    let (prepared, first, second) = fixture.comparison();
    assert!(matches!(
        source_owner().prepare_boolean_answer(
            prepared.challenge(),
            fixture.context(),
            prepared.base_commitment()
        ),
        Err(CheckError::KeyDomainMismatch)
    ));
    let pending = target_owner()
        .prepare_boolean_answer(
            prepared.challenge(),
            fixture.context(),
            prepared.base_commitment(),
        )
        .unwrap();
    let mut commitment = pending.commitment();
    let answer = pending.reveal(prepared.base_commitment()).unwrap();

    // The R1<=127 endpoint spacing cannot be exactly 128. This flip is always invalid.
    let forged = answer.value_for_protocol() ^ 128;
    let mut encoding = Encoder::new(Vec::new());
    encoding
        .array(3)
        .unwrap()
        .str("MASKED_ANSWER_BOOL_SPACED_XOR8_V3")
        .unwrap()
        .bytes(&fixture.context().id())
        .unwrap()
        .u64(forged)
        .unwrap();
    commitment.digest = Sha256::digest(encoding.into_writer()).into();
    let forged = MaskedBooleanAnswer::from_parts(
        fixture.context().id(),
        prepared.challenge().id(),
        forged,
        fixture.context(),
    )
    .unwrap();
    assert!(matches!(
        fixture
            .session
            .decode_boolean(prepared, commitment, forged, first.open(), second.open()),
        Err(CheckError::InvalidBooleanAnswer)
    ));
}

#[test]
fn out_of_width_answers_and_wrong_audit_capability_are_rejected() {
    let fixture = Fixture::new(Width::U8, 1, 0, Comparison::Greater);
    assert!(matches!(
        MaskedBooleanAnswer::from_parts(fixture.context().id(), [0; 32], 256, fixture.context()),
        Err(CheckError::AnswerOutOfRange)
    ));
    let audit = fixture.decode().into_audit_opening();
    assert!(matches!(
        source_owner().check_boolean_audit(&audit, fixture.context()),
        Err(CheckError::KeyDomainMismatch)
    ));
}
