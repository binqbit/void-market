use std::sync::OnceLock;
use tfhe_math::{
    FlaggedValue, KeySet, MathParameters, MathSession, PublicValue, TfheValue, Visibility, Width,
};

pub fn keys() -> &'static KeySet {
    static KEYS: OnceLock<KeySet> = OnceLock::new();
    KEYS.get_or_init(|| KeySet::generate(MathParameters).expect("native key generation"))
}

pub fn foreign_keys() -> &'static KeySet {
    static KEYS: OnceLock<KeySet> = OnceLock::new();
    KEYS.get_or_init(|| KeySet::generate(MathParameters).expect("foreign native key generation"))
}

pub fn session() -> MathSession {
    MathSession::encrypted(keys().evaluation_context())
}

pub fn public(w: Width, n: u64) -> TfheValue {
    PublicValue::uint(w, n).unwrap().into()
}

pub fn encrypted(w: Width, n: u64) -> TfheValue {
    keys()
        .encryption_context()
        .encrypt(PublicValue::uint(w, n).unwrap())
        .unwrap()
}

pub fn boolean(b: bool) -> TfheValue {
    keys()
        .encryption_context()
        .encrypt(PublicValue::boolean(b))
        .unwrap()
}

pub fn uint(v: &TfheValue) -> u64 {
    assert_eq!(v.visibility(), Visibility::Encrypted);
    assert!(v.public_value().is_err());
    keys()
        .decryption_context()
        .decrypt(v)
        .unwrap()
        .as_uint()
        .unwrap()
}

pub fn bool_value(v: &TfheValue) -> bool {
    assert_eq!(v.visibility(), Visibility::Encrypted);
    keys()
        .decryption_context()
        .decrypt(v)
        .unwrap()
        .as_bool()
        .unwrap()
}

pub fn flagged(v: FlaggedValue, n: u64, flag: bool) {
    assert_eq!(uint(&v.value), n);
    assert_eq!(bool_value(&v.flag), flag);
}
