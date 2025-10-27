use core::time::Duration;

use alloy_primitives::address;
use alloy_signer::k256::ecdsa::SigningKey;
use futures::TryStreamExt;
use httpmock::prelude::*;
use serde_json::json;
use url::Url;

use crate::onemoney::epoch_stream;

fn build_epoch_response(epoch_id: u64, consensus_key: &str, addr_hex: &str) -> serde_json::Value {
    json!({
        "epoch_id": epoch_id,
        "hash": format!("0x{:064x}", epoch_id),
        "certificate": {
            "Genesis": {
                "proposal": {
                    "message": {
                        "epoch": { "epoch_id": epoch_id },
                        "chain": 1,
                        "operator_public_key": consensus_key,
                        "operator_address": addr_hex,
                        "validator_set": {
                            "members": [{
                                "consensus_public_key": consensus_key,
                                "address": addr_hex,
                                "peer_id": format!("peer-{epoch_id}"),
                                "archive": false
                            }]
                        }
                    }
                }
            }
        }
    })
}

fn consensus_key_hex() -> String {
    let signing_key = SigningKey::from_bytes(&alloy_signer::k256::Scalar::from(7u64).to_bytes())
        .expect("valid key");
    let verifying_key = *signing_key.verifying_key();
    format!(
        "0x{}",
        hex::encode(verifying_key.to_encoded_point(false).as_bytes())
    )
}

#[tokio::test]
async fn test_epoch_stream_emits_epoch_from_mock() {
    let consensus_key = consensus_key_hex();
    let validator_addr = format!(
        "{:#x}",
        address!("0x0000000000000000000000000000000000000007")
    );

    let server = MockServer::start_async().await;
    let response_body = build_epoch_response(1, &consensus_key, &validator_addr);
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
async fn test_epoch_stream_recovers_after_bad_payload() {
    let consensus_key = consensus_key_hex();
    let validator_addr = format!(
        "{:#x}",
        address!("0x0000000000000000000000000000000000000007")
    );

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
    let success_mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v1/governances/epoch");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(build_epoch_response(1, &consensus_key, &validator_addr));
        })
        .await;

    let result = tokio::time::timeout(Duration::from_secs(2), stream.try_next())
        .await
        .expect("timed out waiting for epoch")
        .expect("stream returned error instead of epoch")
        .expect("no epoch emitted");
    assert_eq!(result.epoch_id, 1);
    success_mock.assert_async().await;
}
