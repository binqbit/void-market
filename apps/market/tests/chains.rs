use market::chains::{ethereum::abi, zcash::memo::MemoBinding};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn ethereum_decoding_rejects_malformed_and_non_ascii_rpc_data() {
    for value in [
        "0x",
        "0x00",
        &format!("0x{}", "é".repeat(32)),
        &format!("0x{}", "g".repeat(64)),
    ] {
        assert!(abi::decoded_owner(value).is_err());
    }
    let address = "0x1234567890123456789012345678901234567890";
    let word = format!("0x{}{}", "0".repeat(24), &address[2..]);
    assert_eq!(abi::decoded_owner(&word).unwrap(), address);
    assert!(abi::owner_of("01").is_err());
    assert!(abi::owner_of("-1").is_err());
    assert!(
        abi::owner_of(
            "115792089237316195423570985008687907853269984665640564039457584007913129639936"
        )
        .is_err()
    );
    assert_eq!(
        abi::token(
            "115792089237316195423570985008687907853269984665640564039457584007913129639935"
        )
        .unwrap(),
        "f".repeat(64)
    );
}

#[test]
fn withdrawal_addresses_reject_zero_and_mistyped_checksums() {
    let valid = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
    assert_eq!(
        abi::checked_address(valid).unwrap(),
        valid.to_ascii_lowercase()
    );
    assert!(abi::checked_address("0x5aaeb6053F3E94C9b9A09f33669435E7Ef1BeAed").is_err());
    assert!(abi::checked_address("0x0000000000000000000000000000000000000000").is_err());
    assert!(abi::checked_address("not-an-address").is_err());
}

#[test]
fn invoice_memo_preserves_wire_offsets_and_reserved_bytes() {
    let memo = MemoBinding {
        obligation_kind: 1,
        intent_class: 3,
        network: [1; 32],
        context: [2; 32],
        obligation: [3; 32],
        invoice: [4; 32],
        issue: [5; 32],
        invoice_nonce: [6; 32],
        intent: [7; 32],
        intent_nonce: [8; 32],
        recipient: [9; 32],
        revision: 17,
        generation: 4,
    }
    .encode()
    .unwrap();
    assert_eq!(&memo[..12], b"\xffVMINV01\x00\x01\x01\x03");
    assert_eq!(&memo[12..16], &[0; 4]);
    assert_eq!(&memo[272..304], &[9; 32]);
    assert_eq!(&memo[304..312], &17u64.to_be_bytes());
    assert_eq!(&memo[312..314], &2u16.to_be_bytes());
    assert_eq!(&memo[314..322], &4u64.to_be_bytes());
    assert!(memo[322..].iter().all(|byte| *byte == 0));
}

#[actix_web::test]
async fn payment_observation_requires_the_exact_confirmed_output() {
    let recipient = uuid::Uuid::new_v4();
    let output = serde_json::json!({
        "pool":"ironwood", "account_uuid":recipient, "address":"uregtest1receiver",
        "walletInternal":false, "outgoing":false, "valueZat":150000000,
        "memo":"expected-invoice", "action":3,
    });
    let valid = serde_json::json!({
        "status":"mined", "blockhash":"canonical", "outputs":[output],
    });
    for change in [
        "none",
        "memo",
        "address",
        "valueZat",
        "walletInternal",
        "account_uuid",
        "duplicate",
        "unconfirmed",
    ] {
        let mut viewed = valid.clone();
        match change {
            "memo" => viewed["outputs"][0]["memo"] = serde_json::json!("another-invoice"),
            "address" => viewed["outputs"][0]["address"] = serde_json::json!("another-receiver"),
            "valueZat" => viewed["outputs"][0]["valueZat"] = serde_json::json!(149999999),
            "walletInternal" => viewed["outputs"][0]["walletInternal"] = serde_json::json!(true),
            "account_uuid" => {
                viewed["outputs"][0]["account_uuid"] = serde_json::json!(uuid::Uuid::new_v4())
            }
            "duplicate" => viewed["outputs"] = serde_json::json!([output, output]),
            _ => {}
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let unconfirmed = change == "unconfirmed";
        let server = actix_web::HttpServer::new(move || {
            let viewed = viewed.clone();
            actix_web::App::new()
                .app_data(actix_web::web::Data::new((viewed, unconfirmed)))
                .route("/", actix_web::web::post().to(observation_rpc))
        })
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let client = market::chains::zcash::Zcash {
            node: market::chains::rpc::Rpc::new(url::Url::parse(&endpoint).unwrap(), None).unwrap(),
            wallet: market::chains::rpc::Rpc::new(url::Url::parse(&endpoint).unwrap(), None)
                .unwrap(),
        };
        let result = client
            .observe_payment(
                "transaction",
                recipient,
                "uregtest1receiver",
                150000000,
                "expected-invoice",
            )
            .await;
        match change {
            "none" => assert!(result.unwrap().is_some()),
            "unconfirmed" => assert!(result.unwrap().is_none()),
            _ => assert!(result.is_err(), "accepted mismatched {change}"),
        }
        handle.stop(true).await;
    }
}

async fn observation_rpc(
    body: actix_web::web::Json<serde_json::Value>,
    data: actix_web::web::Data<(serde_json::Value, bool)>,
) -> actix_web::web::Json<serde_json::Value> {
    let result = match body["method"].as_str().unwrap() {
        "getrawtransaction" => {
            serde_json::json!({"confirmations":if data.1 {2}else{3},"height":17,"blockhash":"canonical"})
        }
        "getblockhash" => serde_json::json!("canonical"),
        "getblockcount" => serde_json::json!(17),
        "getwalletstatus" => {
            serde_json::json!({"wallet_tip":{"height":17},"node_tip":{"height":17},"locked":false})
        }
        "z_viewtransaction" => data.0.clone(),
        other => panic!("Unexpected RPC method {other}"),
    };
    actix_web::web::Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}

#[actix_web::test]
async fn wallet_sync_rejects_a_cached_node_tip() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = actix_web::HttpServer::new(|| {
        actix_web::App::new().route("/", actix_web::web::post().to(stale_tip_rpc))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    actix_web::rt::spawn(server);
    let client = market::chains::zcash::Zcash {
        node: market::chains::rpc::Rpc::new(url::Url::parse(&endpoint).unwrap(), None).unwrap(),
        wallet: market::chains::rpc::Rpc::new(url::Url::parse(&endpoint).unwrap(), None).unwrap(),
    };
    let synchronized = client.synchronized().await.unwrap();
    handle.stop(true).await;
    assert!(
        !synchronized,
        "A wallet's cached node tip is not the live chain tip"
    );
}

async fn stale_tip_rpc(
    body: actix_web::web::Json<serde_json::Value>,
) -> actix_web::web::Json<serde_json::Value> {
    let result = match body["method"].as_str().unwrap() {
        "getwalletstatus" => {
            serde_json::json!({"wallet_tip":{"height":17},"node_tip":{"height":17},"locked":false})
        }
        "getblockcount" => serde_json::json!(18),
        other => panic!("Unexpected RPC method {other}"),
    };
    actix_web::web::Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}

#[actix_web::test]
async fn inconsistent_extraction_can_retry_the_retained_signed_payment() {
    let inconsistent = actix_web::web::Data::new(AtomicBool::new(true));
    let flag = inconsistent.clone();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = url::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .app_data(flag.clone())
            .route("/", actix_web::web::post().to(extraction_rpc))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    actix_web::rt::spawn(server);
    let client = market::chains::zcash::Zcash {
        node: market::chains::rpc::Rpc::new(endpoint.clone(), None).unwrap(),
        wallet: market::chains::rpc::Rpc::new(endpoint, None).unwrap(),
    };

    let first = client.extract("retained-signed-pczt").await;
    inconsistent.store(false, Ordering::Relaxed);
    let retry = client.extract("retained-signed-pczt").await;
    handle.stop(true).await;

    assert!(matches!(
        first,
        Err(market::http::error::ApiError::Unavailable)
    ));
    assert_eq!(
        retry.unwrap(),
        ("expected-id".into(), "signed-bytes".into())
    );
}

async fn extraction_rpc(
    body: actix_web::web::Json<serde_json::Value>,
    inconsistent: actix_web::web::Data<AtomicBool>,
) -> actix_web::web::Json<serde_json::Value> {
    let result = match body["method"].as_str().unwrap() {
        "pczt_extract" => {
            assert_eq!(body["params"], serde_json::json!(["retained-signed-pczt"]));
            serde_json::json!({"txid":"expected-id", "hex":"signed-bytes"})
        }
        "decoderawtransaction" => {
            assert_eq!(body["params"], serde_json::json!(["signed-bytes"]));
            let id = if inconsistent.load(Ordering::Relaxed) {
                "different-id"
            } else {
                "expected-id"
            };
            serde_json::json!({"txid":id})
        }
        other => panic!("Unexpected RPC method {other}"),
    };
    actix_web::web::Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":result}))
}
