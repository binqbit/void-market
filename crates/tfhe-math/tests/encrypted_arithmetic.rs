use super::support::{encrypted, flagged, public, session, uint};
use tfhe_math::{DataType, MathError, Width};

#[test]
fn widening_happens_before_arithmetic() {
    let s = session();
    let a = encrypted(Width::U8, 255);
    let b = encrypted(Width::U8, 255);
    let sum = s.add_wide(&a, &b).unwrap();
    assert_eq!(sum.data_type(), DataType::Uint(Width::U16));
    assert_eq!(uint(&sum), 510);
    let product = s.mul_wide(&a, &b).unwrap();
    assert_eq!(uint(&product), 65025);
    let medium = encrypted(Width::U16, 65535);
    let product = s.mul_wide(&a, &medium).unwrap();
    assert_eq!(product.data_type(), DataType::Uint(Width::U32));
    assert_eq!(uint(&product), 255 * 65535);
    let big = encrypted(Width::U32, u32::MAX.into());
    assert_eq!(
        uint(&s.mul_wide(&big, &big).unwrap()),
        u64::from(u32::MAX).pow(2)
    );
    assert_eq!(
        uint(&s.add_wide(&big, &big).unwrap()),
        2 * u64::from(u32::MAX)
    );
    let max = encrypted(Width::U64, u64::MAX);
    assert!(matches!(
        s.add_wide(&max, &a),
        Err(MathError::UnsupportedWidth { .. })
    ));
    assert!(matches!(
        s.mul_wide(&max, &a),
        Err(MathError::UnsupportedWidth { .. })
    ));
    assert!(matches!(
        s.add_in_width(Width::U8, &encrypted(Width::U16, 1), &a),
        Err(MathError::NarrowingForbidden { .. })
    ));
}

#[test]
fn fixed_candidates_and_secret_flags_cover_all_widths() {
    let s = session();
    for w in Width::ALL {
        let max = encrypted(w, w.maximum());
        let one = public(w, 1);
        let two = public(w, 2);
        flagged(s.add_in_width(w, &max, &one).unwrap(), 0, true);
        flagged(s.sub_in_width(w, &one, &max).unwrap(), 2, true);
        flagged(
            s.mul_in_width(w, &max, &two).unwrap(),
            w.maximum() - 1,
            true,
        );
        flagged(s.add_in_width(w, &encrypted(w, 3), &two).unwrap(), 5, false);
        flagged(
            s.sub_in_width(w, &max, &one).unwrap(),
            w.maximum() - 1,
            false,
        );
        flagged(s.mul_in_width(w, &encrypted(w, 3), &two).unwrap(), 6, false);
    }
    let max = encrypted(Width::U64, u64::MAX);
    flagged(s.mul_in_width(Width::U64, &max, &max).unwrap(), 1, true);
}
