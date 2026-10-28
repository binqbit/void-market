use market::custody::seal::Vault;
use uuid::Uuid;

#[test]
fn custody_records_bind_account_purpose_and_authenticate_ciphertext() {
    let vault = Vault::new(&[8; 32]).unwrap();
    let account = Uuid::new_v4();
    let sealed = vault
        .seal(account, "signing-key", b"fixture secret")
        .unwrap();
    assert_ne!(
        sealed,
        vault
            .seal(account, "signing-key", b"fixture secret")
            .unwrap()
    );
    assert_eq!(
        &*vault.open(account, "signing-key", &sealed).unwrap(),
        b"fixture secret"
    );
    assert!(vault.open(Uuid::new_v4(), "signing-key", &sealed).is_err());
    assert!(vault.open(account, "he-key", &sealed).is_err());
    let mut changed = sealed;
    *changed.last_mut().unwrap() ^= 1;
    assert!(vault.open(account, "signing-key", &changed).is_err());
}
