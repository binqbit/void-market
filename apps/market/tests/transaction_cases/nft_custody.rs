use crate::support::{Fixture, Identity};
use actix_web::{App, HttpServer, dev::ServerHandle, http::StatusCode, web};
use market::chains::{Localnet, config::LocalnetConfig};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const COLLECTION: &str = "0x2222222222222222222222222222222222222222";
const EXTERNAL: &str = "0x3333333333333333333333333333333333333333";

#[derive(Clone)]
struct Chain {
    receipt: Arc<Mutex<Value>>,
    owner: Arc<Mutex<String>>,
}

async fn setup(fixture: &mut Fixture, owner: &Identity) -> (Chain, ServerHandle) {
    let chain = Chain {
        receipt: Arc::new(Mutex::new(Value::Null)),
        owner: Arc::new(Mutex::new(WALLET.into())),
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = url::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let state = chain.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/", web::post().to(rpc))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    actix_web::rt::spawn(server);
    fixture.state.chains = Some(Arc::new(
        Localnet::new(&LocalnetConfig {
            ethereum: endpoint.clone(),
            zcash_node: endpoint.clone(),
            zcash_wallet: endpoint,
            wallet_cookie: "unused-nft-test-cookie".into(),
        })
        .unwrap(),
    ));
    sqlx::query(
        "INSERT INTO demo_accounts(account_id,alias,ethereum_address) VALUES($1,'alice',$2)",
    )
    .bind(owner.address)
    .bind(WALLET)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO localnet_config(singleton,ethereum_genesis,collection_address,collection_code_hash,zcash_genesis,minter_address,faucet_account,faucet_address) VALUES(TRUE,'genesis',$1,'code','zcash',$2,$3,'faucet')")
        .bind(COLLECTION).bind(WALLET).bind(owner.address).execute(&fixture.state.pool).await.unwrap();
    (chain, handle)
}

async fn rpc(body: web::Json<Value>, chain: web::Data<Chain>) -> web::Json<Value> {
    let result = match body["method"].as_str().unwrap() {
        "eth_chainId" => json!("0x7a69"),
        "eth_getBlockByNumber" if body["params"][0] == "0x0" => json!({"hash":"genesis"}),
        "eth_getBlockByNumber" => json!({"hash":"block"}),
        "eth_getTransactionReceipt" => chain.receipt.lock().unwrap().clone(),
        "eth_call" => {
            let data = body["params"][0]["data"].as_str().unwrap();
            if data.starts_with("0x6352211e") {
                json!(format!("0x{:0>64}", &chain.owner.lock().unwrap()[2..]))
            } else {
                json!(format!(
                    "0x{:064x}",
                    if data.contains("ffffffff") { 0 } else { 1 }
                ))
            }
        }
        method => panic!("Unexpected test RPC method {method}"),
    };
    web::Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}

fn receipt(transaction: &str, destination: &str) -> Value {
    json!({"transactionHash":transaction,"blockHash":"block","blockNumber":"0x5","status":"0x1",
    "logs":[{"address":COLLECTION,"topics":[
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        format!("0x{:0>64}",&EXTERNAL[2..]),format!("0x{:0>64}",&destination[2..]),format!("0x{:064x}",42)
    ]}]})
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn withdrawal_binds_an_external_address_and_keeps_the_asset_until_confirmation() {
    let mut fixture = Fixture::with_custody().await;
    let owner = fixture.register("Owner").await;
    let stranger = fixture.register("Stranger").await;
    let (chain, server) = setup(&mut fixture, &owner).await;
    let asset = fixture.asset(&owner).await;
    sqlx::query("UPDATE assets SET nft=$2 WHERE id=$1")
        .bind(asset)
        .bind(json!({"chain_id":31337,"genesis":"genesis","collection":COLLECTION,"token_id":"42"}))
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let path = format!("/v1/assets/{asset}/withdraw");
    let input = json!({"request_id":Uuid::new_v4(),"recipient_address":EXTERNAL});
    assert_eq!(
        fixture
            .request("POST", &path, Some(&stranger.cookie), input.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for recipient in [
        "invalid",
        "0x0000000000000000000000000000000000000000",
        WALLET,
    ] {
        let bad = json!({"request_id":Uuid::new_v4(),"recipient_address":recipient});
        assert_eq!(
            fixture
                .request("POST", &path, Some(&owner.cookie), bad)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let prepared = fixture
        .request("POST", &path, Some(&owner.cookie), input.clone())
        .await;
    assert_eq!(prepared.0, StatusCode::ACCEPTED, "{}", prepared.2);
    assert_eq!(prepared.2["kind"], "withdrawal");
    assert_eq!(prepared.2["nft"]["to"], EXTERNAL);
    assert_eq!(
        fixture
            .request("POST", &path, Some(&owner.cookie), input.clone())
            .await
            .2["id"],
        input["request_id"]
    );
    let mut changed = input.clone();
    changed["recipient_address"] = json!("0x4444444444444444444444444444444444444444");
    assert_eq!(
        fixture
            .request("POST", &path, Some(&owner.cookie), changed)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let owned: Option<Uuid> = sqlx::query_scalar("SELECT owner_id FROM assets WHERE id=$1")
        .bind(asset)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(owned, Some(owner.address));
    let cancel = format!(
        "/v1/transactions/{}/cancel",
        input["request_id"].as_str().unwrap()
    );
    // A failed, already-signed submission must be reconciled before unlocking the NFT.
    let operation: Uuid = serde_json::from_value(input["request_id"].clone()).unwrap();
    sqlx::query(
        "UPDATE chain_operations SET status='failed',signed_transaction='0x1234' WHERE id=$1",
    )
    .bind(operation)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .request("POST", &cancel, Some(&owner.cookie), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    sqlx::query(
        "UPDATE chain_operations SET status='preparing',signed_transaction=NULL WHERE id=$1",
    )
    .bind(operation)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .request("POST", &cancel, Some(&owner.cookie), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    let pending: Option<Uuid> =
        sqlx::query_scalar("SELECT pending_transaction_id FROM assets WHERE id=$1")
            .bind(asset)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(pending, None);

    // A mined revert can be released, but only while the canonical receipt confirms it.
    let request = json!({"request_id":Uuid::new_v4(),"recipient_address":EXTERNAL});
    let prepared = fixture
        .request("POST", &path, Some(&owner.cookie), request.clone())
        .await;
    assert_eq!(prepared.0, StatusCode::ACCEPTED);
    let id: Uuid = serde_json::from_value(request["request_id"].clone()).unwrap();
    let transaction = format!("0x{}", "b".repeat(64));
    sqlx::query("UPDATE chain_operations SET status='failed',signed_transaction='0x1234',transaction_id=$2,error='Ethereum transaction reverted' WHERE id=$1")
        .bind(id).bind(&transaction).execute(&fixture.state.pool).await.unwrap();
    let cancel = format!("/v1/transactions/{id}/cancel");
    assert_eq!(
        fixture
            .request("POST", &cancel, Some(&owner.cookie), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut reverted = receipt(&transaction, WALLET);
    reverted["status"] = json!("0x0");
    *chain.receipt.lock().unwrap() = reverted;
    assert_eq!(
        fixture
            .request("POST", &cancel, Some(&owner.cookie), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    server.stop(true).await;
    fixture.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn deposits_require_real_ownership_and_cannot_credit_a_transaction_twice() {
    let mut fixture = Fixture::with_custody().await;
    let owner = fixture.register("Owner").await;
    let (chain, server) = setup(&mut fixture, &owner).await;
    let transaction = format!("0x{}", "a".repeat(64));
    let input = json!({"request_id":Uuid::new_v4(),"transaction_id":transaction,"collection":COLLECTION,"token_id":"42","name":"My NFT"});
    let address = fixture
        .request(
            "GET",
            "/v1/me/nft-deposit",
            Some(&owner.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(address.2["address"], WALLET);
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                input.clone()
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    *chain.receipt.lock().unwrap() = receipt(&transaction, EXTERNAL);
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                input.clone()
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    *chain.receipt.lock().unwrap() = receipt(&transaction, WALLET);
    *chain.owner.lock().unwrap() = EXTERNAL.into();
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                input.clone()
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    *chain.owner.lock().unwrap() = WALLET.into();
    let deposited = fixture
        .request(
            "POST",
            "/v1/nft-deposits",
            Some(&owner.cookie),
            input.clone(),
        )
        .await;
    assert_eq!(deposited.0, StatusCode::OK, "{}", deposited.2);
    let asset: Uuid = serde_json::from_value(deposited.2["id"].clone()).unwrap();
    assert_eq!(deposited.2["owner_id"], json!(owner.address));
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                input.clone()
            )
            .await
            .2["id"],
        json!(asset)
    );
    let mut duplicate = input.clone();
    duplicate["request_id"] = json!(Uuid::new_v4());
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                duplicate.clone()
            )
            .await
            .0,
        StatusCode::CONFLICT
    );

    // A later return uses a fresh on-chain receipt and preserves the NFT's identity and metadata.
    sqlx::query("UPDATE assets SET owner_id=NULL WHERE id=$1")
        .bind(asset)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .request(
                "POST",
                "/v1/nft-deposits",
                Some(&owner.cookie),
                duplicate.clone()
            )
            .await
            .0,
        StatusCode::CONFLICT
    );
    let returned = format!("0x{}", "b".repeat(64));
    *chain.receipt.lock().unwrap() = receipt(&returned, WALLET);
    duplicate["transaction_id"] = json!(returned);
    let reimported = fixture
        .request("POST", "/v1/nft-deposits", Some(&owner.cookie), duplicate)
        .await;
    assert_eq!(reimported.0, StatusCode::OK, "{}", reimported.2);
    assert_eq!(reimported.2["id"], json!(asset));
    assert_eq!(reimported.2["name"], "My NFT");
    let history = fixture
        .request(
            "GET",
            "/v1/me/transactions",
            Some(&owner.cookie),
            Value::Null,
        )
        .await;
    assert_eq!(history.2.as_array().unwrap().len(), 2);
    assert_eq!(history.2[0]["direction"], "incoming");
    assert_eq!(history.2[0]["can_approve"], false);
    server.stop(true).await;
    fixture.cleanup().await;
}
