use super::support::{bool_value, boolean, encrypted, public, session, uint};
use tfhe_math::Width;

#[test]
fn boolean_truth_table_and_uint_bitwise() {
    let s = session();
    for a in [false, true] {
        for b in [false, true] {
            let x = boolean(a);
            let y = boolean(b);
            assert_eq!(bool_value(&s.xor(&x, &y).unwrap()), a ^ b);
            assert_eq!(bool_value(&s.and(&x, &y).unwrap()), a & b);
            assert_eq!(bool_value(&s.or(&x, &y).unwrap()), a | b);
            assert_eq!(bool_value(&s.not(&x).unwrap()), !a);
        }
    }
    for w in Width::ALL {
        let a = encrypted(w, w.maximum() - 1);
        let b = public(Width::U8, 15);
        assert_eq!(uint(&s.xor(&a, &b).unwrap()), (w.maximum() - 1) ^ 15);
        assert_eq!(uint(&s.and(&a, &b).unwrap()), 14);
        assert_eq!(uint(&s.or(&a, &b).unwrap()), w.maximum());
        assert_eq!(uint(&s.not(&a).unwrap()), 1);
    }
}
