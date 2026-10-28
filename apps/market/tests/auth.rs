use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::JwkSet};
use market::auth::{google::verify_claims, tokens};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// Fixture-only key: unrelated to Google and never trusted by the runtime provider.
const GOOGLE_TEST_KEY: &[u8] = include_bytes!("fixtures/google-test-private.der");

fn google_token(claims: Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key".to_owned());
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_der(GOOGLE_TEST_KEY),
    )
    .unwrap()
}

#[test]
fn jwt_signatures_expiry_and_account_binding() {
    let key = [42; 32];
    let account = Uuid::new_v4();
    let token = tokens::issue(account, &key).unwrap();
    let mut claims = tokens::verify(&token, &key).unwrap();
    assert_eq!(claims.sub, account);
    assert!(tokens::verify(&token, &[41; 32]).is_err());
    assert!(tokens::verify(&format!("{token}tampered"), &key).is_err());

    claims.exp = 1;
    let expired = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(&key),
    )
    .unwrap();
    assert!(tokens::verify(&expired, &key).is_err());
}

#[test]
fn google_requires_valid_signature_issuer_audience_nonce_and_verified_email() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let keys: JwkSet =
        serde_json::from_str(include_str!("fixtures/google-test-jwks.json")).unwrap();
    let claims = json!({
        "iss":"https://accounts.google.com", "aud":"test-client", "azp":"test-client",
        "sub":"google-subject", "email":"member@example.test", "email_verified":true,
        "nonce":"expected-nonce", "iat":now, "exp":now + 300, "name":"Member"
    });
    let token = google_token(claims.clone());
    assert_eq!(
        verify_claims(&token, &keys, "test-client", "expected-nonce")
            .unwrap()
            .sub,
        "google-subject"
    );
    assert!(verify_claims(&token, &keys, "test-client", "wrong-nonce").is_err());
    assert!(verify_claims(&token, &keys, "other-client", "expected-nonce").is_err());
    assert!(verify_claims(&format!("{token}x"), &keys, "test-client", "expected-nonce").is_err());

    for (name, value) in [
        ("iss", json!("https://attacker.test")),
        ("email_verified", json!(false)),
        ("azp", json!("other-client")),
        ("exp", json!(1)),
    ] {
        let mut invalid = claims.clone();
        invalid[name] = value;
        assert!(
            verify_claims(
                &google_token(invalid),
                &keys,
                "test-client",
                "expected-nonce"
            )
            .is_err(),
            "accepted invalid {name}"
        );
    }
}
