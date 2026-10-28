use super::support::{Fixture, target_owner};
use private_check::Comparison;
use tfhe_math::Width;

#[test]
fn encrypted_cross_key_predicates_match_an_independent_oracle() {
    for (predicate, x, v, expected) in [
        (Comparison::Equal, 23, 23, true),
        (Comparison::NotEqual, 23, 23, false),
        (Comparison::Less, 0, 127, true),
        (Comparison::LessOrEqual, 23, 23, true),
        (Comparison::Greater, 23, 23, false),
        (Comparison::GreaterOrEqual, 127, 0, true),
    ] {
        let fixture = Fixture::new(Width::U8, x, v, predicate);
        let decoded = fixture.decode();
        assert_eq!(decoded.value(), expected);
        assert_eq!(decoded.context(), fixture.context().id());
        let audit = decoded.into_audit_opening();
        assert_eq!(
            target_owner()
                .check_boolean_audit(&audit, fixture.context())
                .unwrap(),
            expected
        );
    }
}

#[test]
fn u16_profile_preserves_the_same_relations() {
    let width = Width::U16;
    let fixture = Fixture::new(width, width.maximum() / 2, 1, Comparison::Greater);
    let decoded = fixture.decode();
    assert!(decoded.value());
    assert!(
        target_owner()
            .check_boolean_audit(&decoded.into_audit_opening(), fixture.context())
            .unwrap()
    );
}

#[test]
fn greater_or_equal_preserves_the_inclusive_equal_boundary() {
    let fixture = Fixture::new(Width::U8, 127, 127, Comparison::GreaterOrEqual);
    assert!(fixture.decode().value());
}
