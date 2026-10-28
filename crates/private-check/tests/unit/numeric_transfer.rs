use crate::{
    CheckContext, CheckSession, Comparison, ComputationContext, KeyOwnerContext, NumberInput,
    OwnerId,
};
use tfhe_math::{KeySet, MathParameters, Width};

#[test]
fn full_numeric_roundtrip_preserves_boundary_values_under_independent_keys() {
    let source = KeySet::generate(MathParameters).unwrap();
    let target = KeySet::generate(MathParameters).unwrap();
    let mut owner = [1; 16];
    owner[6] = 0x41;
    owner[8] = 0x81;
    let first = OwnerId::new(owner).unwrap();
    owner[15] = 2;
    let owners = [first, OwnerId::new(owner).unwrap()];
    let helper =
        KeyOwnerContext::new(source.decryption_context(), source.evaluation_context()).unwrap();

    for width in Width::ALL {
        for value in [0, width.maximum() / 2] {
            let input = NumberInput::encrypt(&source.encryption_context(), width, value).unwrap();
            let other = NumberInput::encrypt(&target.encryption_context(), width, 0).unwrap();
            let context =
                CheckContext::new([5; 32], owners, Comparison::Equal, &input, &other).unwrap();
            let session = CheckSession::new(
                context,
                ComputationContext::new(source.encryption_context(), source.evaluation_context())
                    .unwrap(),
                ComputationContext::new(target.encryption_context(), target.evaluation_context())
                    .unwrap(),
            )
            .unwrap();

            let prepared = session.prepare_number_transfer(&input).unwrap();
            let response = helper
                .reencrypt_masked(
                    prepared.challenge(),
                    &target.encryption_context(),
                    session.context(),
                )
                .unwrap();
            let transferred = session.finish_number_transfer(prepared, response).unwrap();

            // Private unit-test access only: production exposes no raw working value.
            assert_eq!(
                target
                    .decryption_context()
                    .decrypt(&transferred.value)
                    .unwrap()
                    .as_uint()
                    .unwrap(),
                value
            );
            assert!(
                !target
                    .decryption_context()
                    .decrypt(&transferred.borrow.value)
                    .unwrap()
                    .as_bool()
                    .unwrap()
            );
        }
    }
}
