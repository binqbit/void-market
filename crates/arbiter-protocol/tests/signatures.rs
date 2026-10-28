use arbiter_protocol::{
    Completion, JobStatus, RequestSignature, SigningKey, public_key, sign, sign_result, verify,
    verify_result,
};
use uuid::Uuid;

#[test]
fn requests_bind_identity_route_body_and_expiry() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let mut request = RequestSignature {
        deployment_id: Uuid::new_v4(),
        node_id: Uuid::new_v4(),
        instance_id: Uuid::new_v4(),
        request_id: Uuid::new_v4(),
        issued_at_ms: 1_000,
        expires_at_ms: 61_000,
        signature: String::new(),
    };
    sign(&key, &mut request, "POST", "/node/v1/register", b"original");
    let public = public_key(&key);
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/register",
            b"original",
            2_000
        )
        .is_ok()
    );
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/register",
            b"changed",
            2_000
        )
        .is_err()
    );
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/heartbeat",
            b"original",
            2_000
        )
        .is_err()
    );
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/register",
            b"original",
            61_000
        )
        .is_err()
    );
    request.node_id = Uuid::new_v4();
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/register",
            b"original",
            2_000
        )
        .is_err()
    );
    request.issued_at_ms = i64::MIN;
    assert!(
        verify(
            &public,
            &request,
            "POST",
            "/node/v1/register",
            b"original",
            2_000
        )
        .is_err()
    );
}

#[test]
fn durable_result_signature_cannot_be_relabelled() {
    let key = SigningKey::from_bytes(&[11; 32]);
    let mut result = Completion {
        deployment_id: Uuid::new_v4(),
        node_id: Uuid::new_v4(),
        instance_id: Uuid::new_v4(),
        id: Uuid::new_v4(),
        job_id: Uuid::new_v4(),
        generation: 1,
        status: JobStatus::Unavailable,
        output: None,
        consumed_response: None,
        signature: String::new(),
    };
    sign_result(&mut result, &key);
    assert!(verify_result(&result, &public_key(&key)).is_ok());
    result.status = JobStatus::Completed;
    assert!(verify_result(&result, &public_key(&key)).is_err());
}
