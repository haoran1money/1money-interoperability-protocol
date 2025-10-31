use std::sync::Once;

use alloy_node_bindings::Anvil;
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use httpmock::prelude::*;
use onemoney_interop::contract::OMInterop;
use relayer::config::Config;
use relayer::outgoing::relay::process_checkpoint_number;
use serde_json::json;

const CHECKPOINT_JSON: &str = include_str!("../src/onemoney/tests/data/checkpoint.json");

static INIT_TRACING: Once = Once::new();

fn init_tracing() {
    INIT_TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt::try_init();
    });
}

fn build_checkpoint_response() -> serde_json::Value {
    serde_json::from_str(CHECKPOINT_JSON).expect("failed to read data/epoch.json")
}

#[tokio::test]
async fn test_process_burn_and_bridge() {
    init_tracing();

    let anvil = Anvil::new().try_spawn().unwrap();

    let keys = anvil.keys();
    let http_endpoint = anvil.endpoint_url();

    let owner_wallet: PrivateKeySigner = keys[0].clone().into();
    let operator_wallet: PrivateKeySigner = keys[1].clone().into();
    let relayer_wallet: PrivateKeySigner = keys[2].clone().into();

    let owner_addr = owner_wallet.address();
    let operator_addr = operator_wallet.address();
    let relayer_addr = relayer_wallet.address();

    let owner_provider = ProviderBuilder::new()
        .wallet(owner_wallet)
        .connect_http(http_endpoint.clone());

    let contract = OMInterop::deploy(
        owner_provider.clone(),
        owner_addr,
        operator_addr,
        relayer_addr,
    )
    .await
    .unwrap();
    let contract_addr = *contract.address();

    let onemoney_server = MockServer::start_async().await;
    let response_body = build_checkpoint_response();
    let mock_checkpoint = onemoney_server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v1/checkpoints/by_number")
                .query_param("number", "1")
                .query_param("full", "true");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(response_body.clone());
        })
        .await;

    let json_nonce_response = json!({
      "nonce": 1
    });

    let mock_account_nonce = onemoney_server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v1/accounts/nonce")
                .query_param("address", "0xc7A8E117Cb43d7935Da4C30B9F9d0cDb5a372808");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json_nonce_response.clone());
        })
        .await;

    let config = Config {
        one_money_node_url: onemoney_server.base_url().parse().unwrap(),
        side_chain_node_url: http_endpoint,
        relayer_private_key: relayer_wallet,
        ominterop_address: contract_addr,
    };

    process_checkpoint_number(&config, 1).await.unwrap();

    // Assert that the mock was called
    mock_checkpoint.assert_async().await;
    mock_account_nonce.assert_async().await;
}
