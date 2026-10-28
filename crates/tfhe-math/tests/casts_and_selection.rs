use super::support::{NATIVE_WIDTHS, bool_value, boolean, encrypted, public, session, uint};
use tfhe_math::{DataType, MathError, Width};

#[test]
fn explicit_casts_and_oblivious_selection() {
    let s = session();
    for w in NATIVE_WIDTHS {
        for b in [false, true] {
            assert_eq!(uint(&s.bool_to_uint(&boolean(b), w).unwrap()), u64::from(b));
        }
        for n in [0, w.maximum()] {
            assert_eq!(bool_value(&s.is_nonzero(&encrypted(w, n)).unwrap()), n != 0);
        }
        assert_eq!(uint(&s.widen(&encrypted(Width::U8, 255), w).unwrap()), 255);
    }
    assert!(matches!(
        s.widen(&encrypted(Width::U16, 1), Width::U8),
        Err(MathError::NarrowingForbidden { .. })
    ));
    assert!(matches!(
        s.bool_to_uint(&encrypted(Width::U8, 1), Width::U8),
        Err(MathError::TypeMismatch)
    ));
    for c in [false, true] {
        let chosen = s
            .select(
                &boolean(c),
                &public(Width::U8, 7),
                &encrypted(Width::U16, 500),
            )
            .unwrap();
        assert_eq!(chosen.data_type(), DataType::Uint(Width::U16));
        assert_eq!(uint(&chosen), if c { 7 } else { 500 });
        assert_eq!(
            bool_value(
                &s.select(&boolean(c), &boolean(true), &boolean(false))
                    .unwrap()
            ),
            c
        );
    }
}
