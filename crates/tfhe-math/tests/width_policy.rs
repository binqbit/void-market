use tfhe_math::{
    MathError, Width,
    width_policy::{addition_width, multiplication_width, widening},
};

#[test]
fn full_width_promotion_table() {
    for a in Width::ALL {
        for b in Width::ALL {
            let add = addition_width(a, b);
            let mul = multiplication_width(a, b);
            assert_eq!(
                add.as_ref().ok().map(|w| w.bits()),
                [8, 16, 32, 64]
                    .into_iter()
                    .find(|w| *w > u16::from(a.max(b).bits()))
                    .map(|w| w as u8)
            );
            assert_eq!(
                mul.as_ref().ok().map(|w| w.bits()),
                [8, 16, 32, 64]
                    .into_iter()
                    .find(|w| *w >= u16::from(a.bits()) + u16::from(b.bits()))
                    .map(|w| w as u8)
            );
        }
    }
}

#[test]
fn narrowing_and_disabled_widths() {
    assert!(matches!(
        widening(Width::U64, Width::U32),
        Err(MathError::NarrowingForbidden { .. })
    ));
    assert!(Width::from_bits(128).is_err());
    assert!(addition_width(Width::U64, Width::U8).is_err());
}
