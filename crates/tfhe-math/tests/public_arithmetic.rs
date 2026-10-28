use tfhe_math::{MathSession, PublicValue, TfheValue, Width};

fn u(w: Width, n: u64) -> TfheValue {
    PublicValue::uint(w, n).unwrap().into()
}

fn n(v: TfheValue) -> u64 {
    v.public_value().unwrap().as_uint().unwrap()
}

#[test]
fn independent_integer_oracle_and_flags() {
    let s = MathSession::public_reference();
    for a in 0..=255 {
        for b in [0, 1, 127, 255] {
            let x = u(Width::U8, a);
            let y = u(Width::U8, b);
            assert_eq!(n(s.add_wide(&x, &y).unwrap()), a + b);
            assert_eq!(n(s.mul_wide(&x, &y).unwrap()), a * b);
            let r = s.add_in_width(Width::U8, &x, &y).unwrap();
            assert_eq!(n(r.value), (a + b) % 256);
            assert_eq!(
                r.flag.public_value().unwrap().as_bool().unwrap(),
                a + b > 255
            );
            let r = s.sub_in_width(Width::U8, &x, &y).unwrap();
            assert_eq!(n(r.value), (a + 256 - b) % 256);
            assert_eq!(r.flag.public_value().unwrap().as_bool().unwrap(), a < b);
            let r = s.mul_in_width(Width::U8, &x, &y).unwrap();
            assert_eq!(n(r.value), (a * b) % 256);
            assert_eq!(
                r.flag.public_value().unwrap().as_bool().unwrap(),
                a * b > 255
            );
        }
    }
}

#[test]
fn sixty_four_bit_boundaries() {
    let s = MathSession::public_reference();
    let max = u(Width::U64, u64::MAX);
    let one = u(Width::U8, 1);
    let r = s.add_in_width(Width::U64, &max, &one).unwrap();
    assert_eq!(n(r.value), 0);
    assert!(r.flag.public_value().unwrap().as_bool().unwrap());
    assert!(s.add_wide(&max, &one).is_err());
    let r = s.mul_in_width(Width::U64, &max, &max).unwrap();
    assert_eq!(n(r.value), 1);
    assert!(r.flag.public_value().unwrap().as_bool().unwrap());
}

#[test]
fn wider_integer_results_preserve_exact_values() {
    let s = MathSession::public_reference();
    let medium = u(Width::U16, u16::MAX.into());
    let big = u(Width::U32, u32::MAX.into());

    assert_eq!(
        n(s.mul_wide(&medium, &medium).unwrap()),
        u64::from(u16::MAX).pow(2)
    );
    assert_eq!(
        n(s.mul_wide(&big, &big).unwrap()),
        u64::from(u32::MAX).pow(2)
    );
    assert_eq!(n(s.add_wide(&big, &big).unwrap()), 2 * u64::from(u32::MAX));
}

#[test]
fn typed_boolean_comparison_and_selection() {
    let s = MathSession::public_reference();
    let yes = TfheValue::from(PublicValue::boolean(true));
    let no = TfheValue::from(PublicValue::boolean(false));
    assert!(s.add_wide(&yes, &u(Width::U8, 1)).is_err());
    assert!(s.lt(&yes, &no).is_err());
    assert_eq!(n(s.bool_to_uint(&yes, Width::U32).unwrap()), 1);
    assert_eq!(
        n(s.select(&no, &u(Width::U8, 2), &u(Width::U32, 300))
            .unwrap()),
        300
    );
    assert!(PublicValue::uint(Width::U8, 256).is_err());
    assert!(s.widen(&u(Width::U32, 7), Width::U8).is_err());
}
