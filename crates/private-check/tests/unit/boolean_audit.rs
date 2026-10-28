use crate::checks::commitments;
use crate::{
    CheckContext, CheckError, CheckSession, Comparison, ComputationContext, KeyOwnerContext,
    MaskedBooleanAnswer, NumberInput, OwnerId, PreparedOwnerMask,
};
use tfhe_math::{KeySet, MathParameters, Width};

#[test]
fn audit_rejects_a_consistent_opposite_endpoint_without_honest_decryption() {
    let source = KeySet::generate(MathParameters).unwrap();
    let target = KeySet::generate(MathParameters).unwrap();
    let x = NumberInput::encrypt(&source.encryption_context(), Width::U8, 42).unwrap();
    let v = NumberInput::encrypt(&target.encryption_context(), Width::U8, 17).unwrap();
    let mut owner = [1; 16];
    owner[6] = 0x41;
    owner[8] = 0x81;
    let first_owner = OwnerId::new(owner).unwrap();
    owner[15] = 2;
    let owners = [first_owner, OwnerId::new(owner).unwrap()];
    let context = CheckContext::new([7; 32], owners, Comparison::Greater, &x, &v).unwrap();
    let check = CheckSession::new(
        context,
        ComputationContext::new(source.encryption_context(), source.evaluation_context()).unwrap(),
        ComputationContext::new(target.encryption_context(), target.evaluation_context()).unwrap(),
    )
    .unwrap();
    let source_owner =
        KeyOwnerContext::new(source.decryption_context(), source.evaluation_context()).unwrap();
    let target_owner =
        KeyOwnerContext::new(target.decryption_context(), target.evaluation_context()).unwrap();

    let transfer = check.prepare_number_transfer(&x).unwrap();
    let response = source_owner
        .reencrypt_masked(
            transfer.challenge(),
            &target.encryption_context(),
            check.context(),
        )
        .unwrap();
    let transferred = check.finish_number_transfer(transfer, response).unwrap();
    let masks = check.prepare_boolean_masks().unwrap();
    let endpoint_delta = *masks.masks.offset ^ (*masks.masks.spacing + *masks.masks.offset);
    let first = PreparedOwnerMask::generate(
        owners[0],
        check.context(),
        masks.commitment(),
        &target.encryption_context(),
    )
    .unwrap();
    let second = PreparedOwnerMask::generate(
        owners[1],
        check.context(),
        masks.commitment(),
        &target.encryption_context(),
    )
    .unwrap();
    let prepared = check
        .compare_protected(
            &transferred,
            &v,
            masks,
            first.contribution(),
            second.contribution(),
        )
        .unwrap();
    let pending = target_owner
        .prepare_boolean_answer(
            prepared.challenge(),
            check.context(),
            prepared.base_commitment(),
        )
        .unwrap();
    let mut commitment = pending.commitment();
    let actual = pending.reveal(prepared.base_commitment()).unwrap();

    // Fault injection: the other valid endpoint and its matching hash still lie about Dec(Q).
    let false_answer = actual.value_for_protocol() ^ endpoint_delta;
    commitment.digest = commitments::answer(check.context().id(), Width::U8, false_answer).unwrap();
    let forged = MaskedBooleanAnswer::from_parts(
        check.context().id(),
        prepared.challenge().id(),
        false_answer,
        check.context(),
    )
    .unwrap();
    let decoded = check
        .decode_boolean(prepared, commitment, forged, first.open(), second.open())
        .unwrap();
    assert!(!decoded.value());

    // A key-owning mathematical audit can check the actual output, beyond hash/endpoints.
    assert_eq!(
        target_owner.check_boolean_audit(&decoded.into_audit_opening(), check.context()),
        Err(CheckError::InvalidBooleanAnswer)
    );
}
