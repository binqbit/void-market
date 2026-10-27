use super::support::{bool_value, boolean, encrypted, public, session};
use tfhe_math::{MathError, Width};

#[test]
fn unsigned_order_and_boolean_equality() {
    let s = session();
    for (a, b) in [(0, 1), (255, 255), (65535, 255)] {
        let x = encrypted(Width::U16, a);
        let y = public(Width::U8, b);
        for (actual, expected) in [
            (s.eq(&x, &y), a == b),
            (s.ne(&x, &y), a != b),
            (s.lt(&x, &y), a < b),
            (s.le(&x, &y), a <= b),
            (s.gt(&x, &y), a > b),
            (s.ge(&x, &y), a >= b),
        ] {
            assert_eq!(bool_value(&actual.unwrap()), expected);
        }
    }
    for b in [false, true] {
        assert_eq!(bool_value(&s.eq(&boolean(true), &boolean(b)).unwrap()), b);
        assert_eq!(bool_value(&s.ne(&boolean(true), &boolean(b)).unwrap()), !b);
    }
    assert!(matches!(
        s.lt(&boolean(false), &boolean(true)),
        Err(MathError::TypeMismatch)
    ));
    assert!(matches!(
        s.eq(&boolean(false), &public(Width::U8, 0)),
        Err(MathError::TypeMismatch)
    ));
}
