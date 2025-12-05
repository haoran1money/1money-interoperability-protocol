use core::time::Duration;

use alloy_primitives::Address;
use alloy_signer::k256::ecdsa::VerifyingKey;
use alloy_signer::utils::public_key_to_address;
use futures::TryStreamExt;
use httpmock::prelude::*;
use serde_json::json;
use url::Url;

use crate::onemoney::epoch_stream;
use crate::onemoney::tests::utils::consensus_key;

fn build_epoch_response(
    epoch_id: u64,
    consensus_key: &VerifyingKey,
    operator_address: Address,
) -> serde_json::Value {
    let consensus_key_hex = format!(
        "0x{}",
        hex::encode(consensus_key.to_encoded_point(true).as_bytes())
    );
    json!({
        "epoch_id": epoch_id,
        "certificate_hash": format!("0x{:064x}", epoch_id),
        "certificate": {
            "type": "Genesis",
            "proposal": {
                "message": {
                    "epoch": { "epoch_id": epoch_id },
                    "chain": 1,
                    "special_accounts": {
                        // reuse the same key/address, update if needed
                        "operator_public_key": consensus_key_hex,
                        "operator_address": operator_address,
                        "escrow_account_public_key": consensus_key_hex,
                        "escrow_account_address": operator_address,
                        "pricing_authority_public_key": consensus_key_hex,
                        "pricing_authority_address": operator_address,
                    },
                    "validator_set": {
                        "members": [{
                            "consensus_public_key": consensus_key_hex,
                            "address": operator_address,
                            "peer_id": format!("peer-{epoch_id}"),
                            "archive": false
                        }]
                    }
                }
            }
        }
    })
}

#[tokio::test]
async fn test_epoch_stream_emits_epoch_from_mock() {
    let consensus_key = consensus_key();
    let operator_address = public_key_to_address(&consensus_key);

    let server = MockServer::start_async().await;
    let response_body = build_epoch_response(1, &consensus_key, operator_address);
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v1/governances/epoch");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(response_body.clone());
        })
        .await;

    let url = Url::parse(&server.base_url()).expect("valid base url");
    let mut stream = epoch_stream(url, Duration::from_millis(200));

    let result = tokio::time::timeout(Duration::from_secs(5), stream.try_next())
        .await
        .expect("timed out waiting for epoch");

    let epoch = result.expect("stream error").expect("no epoch emitted");
    assert_eq!(epoch.epoch_id, 1);
    assert_eq!(epoch.validator_set.members.len(), 1);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_epoch_stream_stops_after_bad_payload() {
    let consensus_key = consensus_key();
    let operator_address = public_key_to_address(&consensus_key);

    let server = MockServer::start_async().await;

    let error_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v1/governances/epoch");
            then.status(200)
                .header("content-type", "application/json")
                .body("not-json");
        })
        .await;

    let url = Url::parse(&server.base_url()).expect("valid base url");
    let mut stream = epoch_stream(url.clone(), Duration::from_millis(200));

    // First poll should surface the JSON decode error.
    let err = tokio::time::timeout(Duration::from_secs(5), stream.try_next())
        .await
        .expect("timed out waiting for error")
        .expect_err("expected stream error");
    assert!(
        matches!(err, crate::onemoney::error::Error::Http(_)),
        "unexpected error variant: {err:?}"
    );
    error_mock.assert_async().await;

    server.reset_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v1/governances/epoch");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(build_epoch_response(1, &consensus_key, operator_address));
        })
        .await;

    let result = tokio::time::timeout(Duration::from_secs(2), stream.try_next())
        .await
        .expect("timed out waiting for stream completion");

    assert!(
        result.expect("stream yielded unexpected result").is_none(),
        "expected stream to end after error"
    );

    mock.assert_calls_async(0).await;
}
