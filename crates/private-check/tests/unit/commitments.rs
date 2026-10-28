use super::{answer, base, owner};
use crate::OwnerId;
use tfhe_math::Width;

#[test]
fn owner_commitment_matches_an_independent_canonical_cbor_vector() {
    let mut identity = [1; 16];
    identity[6] = 0x41;
    identity[8] = 0x81;
    let identity = OwnerId::new(identity).unwrap();
    let digest = owner([0x11; 32], Width::U8, identity, 17, &[0x22; 32]).unwrap();
    let expected = [
        0xec, 0x96, 0x5a, 0x50, 0x78, 0x1f, 0x70, 0x47, 0x09, 0x7f, 0x6d, 0x1f, 0xc4, 0x9a, 0xab,
        0x1a, 0x5b, 0x7a, 0xd9, 0x92, 0x11, 0xff, 0xcf, 0xc3, 0x58, 0x48, 0x5e, 0xdf, 0x82, 0x39,
        0x64, 0xcb,
    ];
    assert_eq!(digest, expected);
    assert_ne!(
        digest,
        owner([0x11; 32], Width::U16, identity, 17, &[0x22; 32]).unwrap()
    );
    assert_ne!(
        digest,
        owner([0x11; 32], Width::U8, identity, 17, &[0x23; 32]).unwrap()
    );
}

#[test]
fn base_order_and_answer_width_are_cryptographically_bound() {
    let original = base([1; 32], Width::U8, [b"spacing", b"offset", b"pad"]).unwrap();
    assert_ne!(
        original,
        base([1; 32], Width::U8, [b"offset", b"spacing", b"pad"]).unwrap()
    );
    assert_ne!(
        answer([1; 32], Width::U8, 17).unwrap(),
        answer([1; 32], Width::U16, 17).unwrap()
    );
}
