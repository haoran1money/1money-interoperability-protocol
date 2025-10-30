use httpmock::prelude::*;
use onemoney_protocol::TxPayload;

use crate::onemoney::transaction::get_transactions_from_checkpoint;

const CHECKPOINT_JSON: &str = include_str!("data/checkpoint.json");

fn build_checkpoint_response() -> serde_json::Value {
    serde_json::from_str(CHECKPOINT_JSON).expect("failed to read data/epoch.json")
}

#[tokio::test]
async fn test_get_token_create_tx() {
    let server = MockServer::start_async().await;
    let response_body = build_checkpoint_response();
    let mock = server
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

    let raw_transactions = get_transactions_from_checkpoint(server.base_url(), 1, |tx| {
        matches!(tx.data, TxPayload::TokenCreate { .. })
    })
    .await;

    let transactions = raw_transactions.expect("transactions error");

    assert!(!transactions.is_empty());

    // Assert that the mock was called
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_token_grant_authority_tx() {
    let server = MockServer::start_async().await;
    let response_body = build_checkpoint_response();
    let mock = server
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

    let raw_transactions = get_transactions_from_checkpoint(server.base_url(), 1, |tx| {
        matches!(tx.data, TxPayload::TokenGrantAuthority { .. })
    })
    .await;

    let transactions = raw_transactions.expect("transactions error");

    assert!(!transactions.is_empty());

    // Assert that the mock was called
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_token_mint_tx() {
    let server = MockServer::start_async().await;
    let response_body = build_checkpoint_response();
    let mock = server
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

    let raw_transactions = get_transactions_from_checkpoint(server.base_url(), 1, |tx| {
        matches!(tx.data, TxPayload::TokenMint { .. })
    })
    .await;

    let transactions = raw_transactions.expect("transactions error");

    assert!(!transactions.is_empty());

    // Assert that the mock was called
    mock.assert_async().await;
}

#[tokio::test]
async fn test_get_token_transfer_tx() {
    let server = MockServer::start_async().await;
    let response_body = build_checkpoint_response();
    let mock = server
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

    let raw_transactions = get_transactions_from_checkpoint(server.base_url(), 1, |tx| {
        matches!(tx.data, TxPayload::TokenTransfer { .. })
    })
    .await;

    let transactions = raw_transactions.expect("transactions error");

    assert!(!transactions.is_empty());

    // Assert that the mock was called
    mock.assert_async().await;
}
