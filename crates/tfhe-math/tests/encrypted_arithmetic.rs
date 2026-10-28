use super::support::{NATIVE_WIDTHS, encrypted, flagged, public, session, uint};
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
    assert!(matches!(
        s.add_in_width(Width::U8, &encrypted(Width::U16, 1), &a),
        Err(MathError::NarrowingForbidden { .. })
    ));
}

#[test]
fn fixed_candidates_and_secret_flags_cover_native_widths() {
    let s = session();
    for w in NATIVE_WIDTHS {
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
}
