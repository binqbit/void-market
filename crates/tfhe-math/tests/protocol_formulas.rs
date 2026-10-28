use super::support::{NATIVE_WIDTHS, bool_value, boolean, encrypted, session, uint};

#[test]
fn known_bound_numeric_and_boolean_masks_match_independent_oracle() {
    let s = session();
    for w in NATIVE_WIDTHS {
        let l = w.maximum() / 2;
        let mask = w.maximum() - 3;
        let x = encrypted(w, l);
        let r = encrypted(w, l);
        let xor_mask = encrypted(w, mask);
        let sum = s.add_in_width(w, &x, &r).unwrap();
        assert!(!bool_value(&sum.flag));
        assert_eq!(uint(&s.xor(&sum.value, &xor_mask).unwrap()), (l + l) ^ mask);
        for bit in [false, true] {
            let b = s.bool_to_uint(&boolean(bit), w).unwrap();
            let product = s.mul_in_width(w, &b, &r).unwrap();
            assert!(!bool_value(&product.flag));
            let sum = s.add_in_width(w, &product.value, &x).unwrap();
            assert!(!bool_value(&sum.flag));
            assert_eq!(
                uint(&s.xor(&sum.value, &xor_mask).unwrap()),
                (u64::from(bit) * l + l) ^ mask
            );
        }
    }
}
