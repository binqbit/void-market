mod auction_cases;
mod message_cases;
mod node_cases;
mod support;
mod transaction_cases;

use actix_web::{
    App,
    http::{StatusCode, header},
    test,
};
use image::{ImageEncoder, codecs::png::PngEncoder};
use market::{database, events, state::AppState};
use serde_json::{Value, json};
use support::{Fixture, ORIGIN};
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn demo_login_and_resources_are_disabled_in_the_normal_profile() {
    let fixture = Fixture::new().await;
    let (_, _, config) = fixture
        .request("GET", "/v1/auth/config", None, Value::Null)
        .await;
    assert_eq!(config["demo"], false);
    assert_eq!(
        fixture
            .request("GET", "/v1/demo", None, Value::Null)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request("POST", "/v1/demo/login", None, json!({"actor":"alice"}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn profile_username_uniqueness_and_private_email() {
    let fixture = Fixture::new().await;
    let alice = fixture.register("Alice").await;
    let bob = fixture.register("Bob").await;
    sqlx::query("UPDATE accounts SET email = 'alice@example.test' WHERE id = $1")
        .bind(alice.address)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let update = json!({"name":"Alice", "username":"alice"});
    let (status, _, profile) = fixture
        .request(
            "POST",
            "/v1/me/profile",
            Some(&alice.cookie),
            update.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(profile["email"], "alice@example.test");
    assert_eq!(
        fixture
            .request("POST", "/v1/me/profile", Some(&bob.cookie), update)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/me/profile",
                Some(&alice.cookie),
                json!({"name":"Alice", "username":"another_name"})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (_, _, public) = fixture
        .request(
            "GET",
            &format!("/v1/accounts/{}", alice.address),
            None,
            Value::Null,
        )
        .await;
    assert_eq!(public["username"], "alice");
    assert!(public.get("email").is_none());
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn google_flow_binds_state_to_browser_and_uses_pkce() {
    let mut fixture = Fixture::new().await;
    std::sync::Arc::get_mut(&mut fixture.state.config)
        .unwrap()
        .google = Some(market::config::GoogleConfig {
        client_id: "test-client".to_owned(),
        client_secret: "fixture-only-secret".to_owned(),
    });
    let (status, headers, _) = fixture
        .request("GET", "/v1/auth/google/start", None, Value::Null)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let url = url::Url::parse(headers.get(header::LOCATION).unwrap().to_str().unwrap()).unwrap();
    let parameters = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(parameters["code_challenge_method"], "S256");
    assert_eq!(
        parameters["redirect_uri"],
        "http://localhost:3000/api/v1/auth/google/callback"
    );
    assert_ne!(parameters["state"], parameters["nonce"]);
    let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    let state = &parameters["state"];
    let callback = format!("/v1/auth/google/callback?state={state}&error=access_denied");
    let (_, response, _) = fixture.request("GET", &callback, None, Value::Null).await;
    assert_eq!(
        response.get(header::LOCATION).unwrap(),
        "/login?error=google"
    );
    let (_, response, _) = fixture
        .request(
            "GET",
            &callback,
            Some(cookie.split(';').next().unwrap()),
            Value::Null,
        )
        .await;
    assert_eq!(
        response.get(header::LOCATION).unwrap(),
        "/login?error=google"
    );
    assert!(
        response
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    for (next, retained) in [
        ("/market/example", true),
        ("//example.org", false),
        ("https://example.org", false),
    ] {
        let path = format!(
            "/v1/auth/google/start?next={}",
            url::form_urlencoded::byte_serialize(next.as_bytes()).collect::<String>()
        );
        let (_, headers, _) = fixture.request("GET", &path, None, Value::Null).await;
        let url =
            url::Url::parse(headers.get(header::LOCATION).unwrap().to_str().unwrap()).unwrap();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let cookie = headers
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let (_, response, _) = fixture
            .request(
                "GET",
                &format!("/v1/auth/google/callback?state={state}&error=access_denied"),
                Some(cookie),
                Value::Null,
            )
            .await;
        let location = response.get(header::LOCATION).unwrap().to_str().unwrap();
        assert_eq!(location.contains("next="), retained);
        assert!(location.starts_with("/login?error=google"));
    }
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn ownership_transfer_retries_authorization_and_restart() {
    let mut fixture = Fixture::new().await;
    let alice = fixture.register("Alice").await;
    let bob = fixture.register("Bob").await;
    let charlie = fixture.register("Charlie").await;
    let asset = fixture.asset(&alice).await;
    let path = format!("/v1/assets/{asset}/transfers");
    let request_id = Uuid::new_v4();
    let transfer = json!({"request_id":request_id,"recipient_address":bob.address});

    assert_eq!(
        fixture
            .request("POST", &path, Some(&bob.cookie), transfer.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &path,
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"recipient_address":alice.address})
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &path,
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"recipient_address":Uuid::new_v4()})
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    let (status, _, first) = fixture
        .request("POST", &path, Some(&alice.cookie), transfer.clone())
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (_, _, retry) = fixture
        .request("POST", &path, Some(&alice.cookie), transfer)
        .await;
    assert_eq!(first, retry);
    assert_eq!(
        fixture
            .request(
                "POST",
                &path,
                Some(&alice.cookie),
                json!({"request_id":request_id,"recipient_address":charlie.address})
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                &path,
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"recipient_address":charlie.address})
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let (status, _, _) = fixture
        .request(
            "POST",
            "/v1/assets",
            Some(&alice.cookie),
            json!({"request_id":asset,"name":"Registered asset"}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    fixture.state = AppState::new(
        fixture.state.pool.clone(),
        market::config::Config {
            localnet: None,
            custody: None,
            database: fixture.state.config.database.clone(),
            listen: fixture.state.config.listen,
            origin: fixture.state.config.origin.clone(),
            rp_id: "localhost".to_owned(),
            secure_cookie: false,
            media_root: fixture.state.config.media_root.clone(),
            kafka_brokers: fixture.state.config.kafka_brokers.clone(),
            event_topic: fixture.state.config.event_topic.clone(),
            node_event_topic: fixture.state.config.node_event_topic.clone(),
            jwt_secret: fixture.state.config.jwt_secret.clone(),
            google: None,
        },
    )
    .unwrap();
    let (status, _, body) = fixture
        .request("GET", &format!("/v1/assets/{asset}"), None, Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["owner_id"], json!(bob.address));
    assert_eq!(
        fixture
            .request(
                "GET",
                &format!("/v1/transfers/{request_id}"),
                Some(&charlie.cookie),
                Value::Null
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM transfers")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(database::ready(&fixture.state.pool).await);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn concurrent_transfers_and_duplicate_requests_are_atomic() {
    let fixture = Fixture::new().await;
    let alice = fixture.register("Alice").await;
    let bob = fixture.register("Bob").await;
    let charlie = fixture.register("Charlie").await;
    let asset = fixture.asset(&alice).await;
    let path = format!("/v1/assets/{asset}/transfers");
    let (first, second) = tokio::join!(
        fixture.request(
            "POST",
            &path,
            Some(&alice.cookie),
            json!({"request_id":Uuid::new_v4(),"recipient_address":bob.address})
        ),
        fixture.request(
            "POST",
            &path,
            Some(&alice.cookie),
            json!({"request_id":Uuid::new_v4(),"recipient_address":charlie.address})
        )
    );
    assert_eq!(
        [first.0, second.0]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        [first.0, second.0]
            .iter()
            .filter(|s| **s == StatusCode::FORBIDDEN)
            .count(),
        1
    );

    let other = fixture.asset(&alice).await;
    let path = format!("/v1/assets/{other}/transfers");
    let input = json!({"request_id":Uuid::new_v4(),"recipient_address":bob.address});
    let (first, second) = tokio::join!(
        fixture.request("POST", &path, Some(&alice.cookie), input.clone()),
        fixture.request("POST", &path, Some(&alice.cookie), input)
    );
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.2, second.2);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM transfers WHERE asset_id = $1")
        .bind(other)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn sessions_origin_checks_and_single_use_passkey_ceremonies() {
    let fixture = Fixture::new().await;
    let mut alice = fixture.register("Alice").await;
    assert_eq!(
        fixture
            .request("GET", "/v1/me", Some(&alice.cookie), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/assets",
                None,
                json!({"request_id":Uuid::new_v4(),"name":"asset"})
            )
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fixture
            .request_origin(
                "POST",
                "/v1/assets",
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"name":"asset"}),
                "https://other.example"
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    fixture
        .request("POST", "/v1/auth/logout", Some(&alice.cookie), Value::Null)
        .await;
    assert_eq!(
        fixture
            .request("GET", "/v1/me", Some(&alice.cookie), Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    fixture.login(&mut alice).await;

    let (_, _, options) = fixture
        .request(
            "POST",
            "/v1/auth/login/options",
            None,
            json!({"address":alice.address}),
        )
        .await;
    let credential = alice
        .authenticator
        .do_authentication(
            url::Url::parse(ORIGIN).unwrap(),
            serde_json::from_value(options["public_key"].clone()).unwrap(),
        )
        .unwrap();
    let input = json!({"ceremony_id":options["ceremony_id"],"credential":credential});
    assert_eq!(
        fixture
            .request("POST", "/v1/auth/login/finish", None, input.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .request("POST", "/v1/auth/login/finish", None, input)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/assets",
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"name":"asset","owner_id":Uuid::new_v4()})
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn image_upload_is_bounded_and_does_not_accept_foreign_paths() {
    let fixture = Fixture::new().await;
    let alice = fixture.register("Alice").await;
    let bob = fixture.register("Bob").await;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let mut body = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"image.png\"\r\nContent-Type: image/png\r\n\r\n".to_vec();
    body.extend(png);
    body.extend(b"\r\n--boundary--\r\n");
    let request = test::TestRequest::post()
        .uri("/v1/media")
        .insert_header((header::ORIGIN, ORIGIN))
        .insert_header((header::COOKIE, alice.cookie.as_str()))
        .insert_header((
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=boundary",
        ))
        .set_payload(body)
        .to_request();
    let app = test::init_service(
        App::new().configure(market::http::router::configure(fixture.state.clone())),
    )
    .await;
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let image: Value = serde_json::from_slice(&test::read_body(response).await).unwrap();
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/assets",
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"name":"image","image":image["image"]})
            )
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/assets",
                Some(&bob.cookie),
                json!({"request_id":Uuid::new_v4(),"name":"image","image":image["image"]})
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/assets",
                Some(&alice.cookie),
                json!({"request_id":Uuid::new_v4(),"name":"image","image":"../../etc/passwd"})
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture
            .request(
                "GET",
                "/v1/me/assets?limit=1000",
                Some(&alice.cookie),
                Value::Null
            )
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL and TEST_KAFKA_BROKERS"]
async fn broker_failure_keeps_outbox_events_for_retry() {
    let fixture = Fixture::new().await;
    let alice = fixture.register("Alice").await;
    fixture.asset(&alice).await;
    let offline = events::producer("127.0.0.1:1").unwrap();
    assert!(
        events::deliver_batch(&fixture.state, &offline)
            .await
            .is_err()
    );
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox WHERE published_at IS NULL")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(pending, 1);
    let online = events::producer(&fixture.state.config.kafka_brokers).unwrap();
    assert_eq!(
        events::deliver_batch(&fixture.state, &online)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        events::deliver_batch(&fixture.state, &online)
            .await
            .unwrap(),
        0
    );
    fixture.cleanup().await;
}
