use super::support::{Fixture, owners, target_keys, target_owner};
use private_check::{CheckError, Comparison, OwnerMaskOpening, PreparedOwnerMask};
use tfhe_math::Width;

#[test]
fn owner_openings_bind_value_identity_context_and_original_base() {
    let fixture = Fixture::new(Width::U8, 3, 4, Comparison::Less);
    let base = fixture.session.prepare_boolean_masks().unwrap();
    let owner = PreparedOwnerMask::generate(
        owners()[0],
        fixture.context(),
        base.commitment(),
        &target_keys().encryption_context(),
    )
    .unwrap();
    let opening = owner.open();
    let (pad, rho) = opening.expose();
    assert!(matches!(
        OwnerMaskOpening::from_parts(opening.binding(), pad ^ 1, *rho, fixture.context()),
        Err(CheckError::CommitmentMismatch)
    ));

    let mut wrong_owner = opening.binding();
    wrong_owner.owner = owners()[1];
    assert!(matches!(
        OwnerMaskOpening::from_parts(wrong_owner, pad, *rho, fixture.context()),
        Err(CheckError::CommitmentMismatch)
    ));
    let mut wrong_context = opening.binding();
    wrong_context.context = [0; 32];
    assert!(matches!(
        OwnerMaskOpening::from_parts(wrong_context, pad, *rho, fixture.context()),
        Err(CheckError::ContextMismatch)
    ));
}

#[test]
fn unchanged_base_commitment_is_required_before_answer_reveal() {
    let fixture = Fixture::new(Width::U8, 3, 4, Comparison::Less);
    let (prepared, _, _) = fixture.comparison();
    assert!(matches!(
        target_owner().prepare_boolean_answer(prepared.challenge(), fixture.context(), [0; 32]),
        Err(CheckError::CommitmentMismatch)
    ));
    let pending = target_owner()
        .prepare_boolean_answer(
            prepared.challenge(),
            fixture.context(),
            prepared.base_commitment(),
        )
        .unwrap();
    assert!(matches!(
        pending.reveal([0; 32]),
        Err(CheckError::CommitmentMismatch)
    ));
}

#[test]
fn answer_commitment_mismatch_never_becomes_false() {
    let fixture = Fixture::new(Width::U8, 3, 4, Comparison::Less);
    let (prepared, first, second) = fixture.comparison();
    let pending = target_owner()
        .prepare_boolean_answer(
            prepared.challenge(),
            fixture.context(),
            prepared.base_commitment(),
        )
        .unwrap();
    let mut commitment = pending.commitment();
    commitment.digest[0] ^= 1;
    let answer = pending.reveal(prepared.base_commitment()).unwrap();
    assert!(matches!(
        fixture
            .session
            .decode_boolean(prepared, commitment, answer, first.open(), second.open()),
        Err(CheckError::CommitmentMismatch)
    ));
}
