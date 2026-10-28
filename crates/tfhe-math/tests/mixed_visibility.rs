use super::support::{bool_value, encrypted, flagged, public, session, uint};
use tfhe_math::{MathError, MathSession, PublicValue, Width};

#[test]
fn mixed_operands_preserve_order_and_no_plaintext_fallback() {
    let s = session();
    let a = encrypted(Width::U8, 9);
    let b = public(Width::U8, 20);
    assert_eq!(uint(&s.add_wide(&a, &b).unwrap()), 29);
    assert_eq!(uint(&s.add_wide(&b, &a).unwrap()), 29);
    flagged(s.sub_in_width(Width::U16, &a, &b).unwrap(), 65525, true);
    flagged(s.sub_in_width(Width::U16, &b, &a).unwrap(), 11, false);
    assert!(bool_value(&s.lt(&a, &b).unwrap()));
    assert!(!bool_value(&s.lt(&b, &a).unwrap()));
    assert!(matches!(
        s.add_wide(&b, &b),
        Err(MathError::BackendMismatch)
    ));
    assert!(matches!(s.not(&b), Err(MathError::BackendMismatch)));
    let reference = MathSession::public_reference();
    assert!(matches!(
        reference.add_wide(&a, &b),
        Err(MathError::BackendMismatch)
    ));
    assert!(matches!(
        reference.select(&PublicValue::boolean(false).into(), &a, &b),
        Err(MathError::BackendMismatch)
    ));
    // A public condition must not discard an encrypted, unselected branch's domain/visibility.
    assert_eq!(
        uint(
            &s.select(&PublicValue::boolean(false).into(), &a, &b)
                .unwrap()
        ),
        20
    );
}
