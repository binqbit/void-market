use super::support::{NATIVE_WIDTHS, encrypted, foreign_keys, keys, public, session, uint};
use sha2::{Digest, Sha256};
use tfhe_math::{
    DataType, EncryptionContext, EvaluationContext, MathError, MathSession, PublicValue, Result,
    TfheValue, Width,
    codec::{
        CIPHERTEXT_LIMIT, EVALUATION_KEY_LIMIT, EncodingLimits, PUBLIC_KEY_LIMIT, encrypt_compact,
        export_ciphertext, export_evaluation_key, export_public_key, import_ciphertext,
        import_compact, import_evaluation_key, import_public_key,
    },
};

const HEADER: usize = 148;
fn digest(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn import(b: &[u8], ty: DataType) -> Result<TfheValue> {
    import_ciphertext(
        b,
        digest(b),
        ty,
        &keys().evaluation_context(),
        EncodingLimits::default(),
    )
}

fn replace_body(frame: &[u8], body: &[u8]) -> Vec<u8> {
    let mut bytes = frame[..HEADER].to_vec();
    bytes[12..20].copy_from_slice(&(body.len() as u64).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}

#[test]
fn expanded_and_compact_roundtrips_for_native_test_types() {
    let limits = EncodingLimits::default();
    let ctx = keys().encryption_context();
    let values = std::iter::once(PublicValue::boolean(true)).chain(
        NATIVE_WIDTHS
            .into_iter()
            .map(|w| PublicValue::uint(w, w.maximum()).unwrap()),
    );
    for plain in values {
        let value = ctx.encrypt(plain).unwrap();
        let encoded = export_ciphertext(&value, limits).unwrap();
        assert_eq!(encoded.digest(), digest(encoded.as_bytes()));
        let restored = import(encoded.as_bytes(), plain.data_type()).unwrap();
        assert_eq!(
            keys().decryption_context().decrypt(&restored).unwrap(),
            plain
        );
        let compact = encrypt_compact(&ctx, plain, limits).unwrap();
        let restored = import_compact(
            compact.as_bytes(),
            compact.digest(),
            plain.data_type(),
            &ctx,
            limits,
        )
        .unwrap();
        assert_eq!(
            keys().decryption_context().decrypt(&restored).unwrap(),
            plain
        );
    }
}

#[test]
fn keys_roundtrip_and_retain_exact_binding() {
    let limits = EncodingLimits::default();
    let binding = keys().binding();
    let p = export_public_key(keys().public_key(), limits).unwrap();
    let public = import_public_key(p.as_bytes(), binding, limits).unwrap();
    let e = export_evaluation_key(keys().evaluation_key(), limits).unwrap();
    let evaluation = import_evaluation_key(e.as_bytes(), binding, limits).unwrap();
    let value = EncryptionContext::new(public)
        .encrypt(PublicValue::uint(Width::U8, 9).unwrap())
        .unwrap();
    let s = MathSession::encrypted(EvaluationContext::new(evaluation));
    assert_eq!(uint(&s.not(&value).unwrap()), 246);
    let mut bad = p.as_bytes().to_vec();
    *bad.last_mut().unwrap() ^= 1;
    assert!(matches!(
        import_public_key(&bad, binding, limits),
        Err(MathError::InvalidEncoding)
    ));
    assert!(matches!(
        import_public_key(p.as_bytes(), foreign_keys().binding(), limits),
        Err(MathError::KeyDomainMismatch)
    ));
    assert!(matches!(
        import_evaluation_key(p.as_bytes(), binding, limits),
        Err(MathError::InvalidEncoding)
    ));
}

#[test]
fn malformed_frames_hashes_types_domains_and_limits_are_rejected() {
    let ty = DataType::Uint(Width::U8);
    let limits = EncodingLimits::default();
    let encoded = export_ciphertext(&encrypted(Width::U8, 42), limits).unwrap();
    let original = encoded.as_bytes();
    assert_eq!(&original[..8], b"TFHEMATH");

    // Reject the previous project-prefixed frame marker.
    let mut legacy_header = original.to_vec();
    legacy_header[..2].copy_from_slice(b"VM");
    assert!(matches!(
        import(&legacy_header, ty),
        Err(MathError::InvalidEncoding)
    ));

    for len in [0, 8, 20, HEADER - 1, original.len() - 1] {
        assert!(import(&original[..len], ty).is_err());
    }
    for index in [0, 8, 10, 11, 12, 20, 52, 84, 116] {
        let mut bad = original.to_vec();
        bad[index] ^= 1;
        assert!(import(&bad, ty).is_err());
    }
    assert!(matches!(
        import_ciphertext(original, [0; 32], ty, &keys().evaluation_context(), limits),
        Err(MathError::InvalidEncoding)
    ));
    assert!(matches!(
        import(original, DataType::Uint(Width::U16)),
        Err(MathError::InvalidEncoding)
    ));
    // Header relabelling must not bypass the native type/conformance checks.
    let mut relabelled = original.to_vec();
    relabelled[11] = 3;
    assert!(import(&relabelled, DataType::Uint(Width::U16)).is_err());
    assert!(matches!(
        import_ciphertext(
            original,
            encoded.digest(),
            ty,
            &foreign_keys().evaluation_context(),
            limits
        ),
        Err(MathError::KeyDomainMismatch)
    ));
    let mut trailing = original.to_vec();
    trailing.push(0);
    assert!(import(&trailing, ty).is_err());
    let trailing = replace_body(original, &[&original[HEADER..], &[0]].concat());
    assert!(import(&trailing, ty).is_err());
    let tiny = EncodingLimits::new(149, PUBLIC_KEY_LIMIT, EVALUATION_KEY_LIMIT).unwrap();
    assert!(matches!(
        export_ciphertext(&encrypted(Width::U8, 1), tiny),
        Err(MathError::ResourceLimit)
    ));
    assert!(matches!(
        import_ciphertext(
            original,
            encoded.digest(),
            ty,
            &keys().evaluation_context(),
            tiny
        ),
        Err(MathError::ResourceLimit)
    ));
    assert!(
        EncodingLimits::new(CIPHERTEXT_LIMIT + 1, PUBLIC_KEY_LIMIT, EVALUATION_KEY_LIMIT).is_err()
    );
    assert!(EncodingLimits::new(148, PUBLIC_KEY_LIMIT, EVALUATION_KEY_LIMIT).is_err());
    assert!(matches!(
        export_ciphertext(&public(Width::U8, 1), limits),
        Err(MathError::BackendMismatch)
    ));
}

#[test]
fn packed_multi_element_and_wrong_native_compact_types_are_rejected_without_keyed_retry() {
    let limits = EncodingLimits::default();
    let ctx = keys().encryption_context();
    let p = export_public_key(keys().public_key(), limits).unwrap();
    let config = tfhe::ConfigBuilder::with_custom_parameters(
        tfhe::shortint::parameters::PARAM_MESSAGE_2_CARRY_2_KS_PBS_TUNIFORM_2M128,
    )
    .use_dedicated_oprf_key(false)
    .build();
    let params = config.public_key_encryption_parameters().unwrap();
    let raw: tfhe::CompactPublicKey = tfhe::safe_serialization::safe_deserialize_conformant(
        &p.as_bytes()[HEADER..],
        PUBLIC_KEY_LIMIT,
        &params,
    )
    .unwrap();
    let original = encrypt_compact(&ctx, PublicValue::uint(Width::U8, 3).unwrap(), limits).unwrap();
    let mut packed = tfhe::CompactCiphertextList::builder(&raw);
    packed.push(3u8);
    let mut multiple = tfhe::CompactCiphertextList::builder(&raw);
    multiple.push(3u8).push(4u8);
    let mut wrong = tfhe::CompactCiphertextList::builder(&raw);
    wrong.push(3u16);
    for list in [packed.build_packed(), multiple.build(), wrong.build()] {
        let mut body = Vec::new();
        tfhe::safe_serialization::safe_serialize(&list, &mut body, CIPHERTEXT_LIMIT).unwrap();
        let bad = replace_body(original.as_bytes(), &body);
        assert!(
            import_compact(&bad, digest(&bad), DataType::Uint(Width::U8), &ctx, limits).is_err()
        );
    }
    // A normal operation afterwards confirms the rejected input did not poison context state.
    assert_eq!(uint(&session().not(&encrypted(Width::U8, 3)).unwrap()), 252);
}
