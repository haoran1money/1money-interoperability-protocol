use httpmock::prelude::*;
use onemoney_protocol::TxPayload;
use serde_json::json;

use crate::onemoney::transaction::get_transactions_from_checkpoint;

fn build_checkpoint_response() -> serde_json::Value {
    json!({
        "hash": "0x8766283f8a9ec1e916907a47f8f4e419784d13b2470850b37fcd6cfd8728842b",
        "parent_hash": "0x91add6d27003a14ae9ee5031dc16dc4f809d143692192c20c7451ce2ce8e2458",
        "state_root": "0xd7ca67d07c2795f94e508a294d34ef1dbd6ba5816189ceebc97212cf71f9ec9f",
        "transactions_root": "0x9baa4fd5bfe214873f51beb4eb3cf246dbd57472fbe3ef999dfd87ef5e6e6dce",
        "receipts_root": "0x793a1002564dd12d76c3136baa6fcb88aa791c726bd57a9c2636811f6f855fcc",
        "number": 1,
        "timestamp": 1760175374,
        "extra_data": "",
        "transactions": [
            {
                "hash": "0x0d44dd486e931778626f2a1354d40663c39da6df228ae499f9b4a50ec7f99c7b",
                "checkpoint_hash": "0x8766283f8a9ec1e916907a47f8f4e419784d13b2470850b37fcd6cfd8728842b",
                "checkpoint_number": 1,
                "transaction_index": 0,
                "recent_checkpoint": 0,
                "chain_id": 1212101,
                "from": "0xa634dfba8c7550550817898bc4820cd10888aac5",
                "nonce": 0,
                "transaction_type": "TokenCreate",
                "data": {
                    "symbol": "1USD175372",
                    "decimals": 6,
                    "master_authority": "0xa634dfba8c7550550817898bc4820cd10888aac5",
                    "is_private": false,
                    "name": "1USD175372"
                },
                "signature": {
                    "r": "0x9ec093436f8a3e007762560d4c036d83b8a0f05c17e934ace9fa7e426fec5759",
                    "s": "0x793c2dee3e10645a43278eb0fcacc9d47ed257dfe5e3004eb768b2af6d251623",
                    "v": 0
                }
            },
            {
                "hash": "0xefc4bc6ae4efa16249080ef1b511cf778f1d0ab9e96ed9109e6b2f14f3cee999",
                "checkpoint_hash": "0x8766283f8a9ec1e916907a47f8f4e419784d13b2470850b37fcd6cfd8728842b",
                "checkpoint_number": 1,
                "transaction_index": 1,
                "recent_checkpoint": 0,
                "chain_id": 1212101,
                "from": "0xa634dfba8c7550550817898bc4820cd10888aac5",
                "nonce": 1,
                "transaction_type": "TokenGrantAuthority",
                "data": {
                    "authority_type": "MintBurnTokens",
                    "authority_address": "0x1ffda73cfce533b1054d274bfad46ff9b0601b5c",
                    "value": "1000000000000000000",
                    "token": "0xf864012249f6843fbdc1eb0d55aff9252c09cef8"
                },
                "signature": {
                    "r": "0x87847590565a9539c870e47a3e107ab263404937083c7f78df10f50daef15678",
                    "s": "0x5a0cce3edcf28649475c6fdb765e2f5cf4cfea11989fef1878ad873733b4d13f",
                    "v": 1
                }
            },
            {
                "hash": "0x1fe3c4deb49a25ac50c7bd8822638e54447ac31b12d7f3a581d6b1f29a23cf70",
                "checkpoint_hash": "0x691c40cc513ed3e34bca9601271382662f4256b7eac0c74ed1fdbdd00c539cde",
                "checkpoint_number": 3,
                "transaction_index": 4,
                "recent_checkpoint": 2,
                "chain_id": 1212101,
                "from": "0x8a26d6017999386126287b61566bba1a93822ce6",
                "nonce": 1,
                "transaction_type": "TokenMint",
                "data": {
                    "value": "1000000000",
                    "recipient": "0xf66b427e28b48f507f16470643e80b17616cba81",
                    "token": "0xf864012249f6843fbdc1eb0d55aff9252c09cef8"
                },
                "signature": {
                    "r": "0x84db41f518218125306434269cb87e77b2f76ccefaa734b66271a141bab03a82",
                    "s": "0x57491671c5e596bcc5ae0c973523704b194cc9ade2eb36a723f8ef8d7636eb66",
                    "v": 0
                }
            },
            {
                "hash": "0x05e0238f5f130a41d1102d9e532e338eac86f6d1bf427e39fb9e427598c64034",
                "checkpoint_hash": "0x06a9d1306d4d79e880328be616b04b16d4dca7582b4cfedd83459e839b897035",
                "checkpoint_number": 7,
                "transaction_index": 0,
                "recent_checkpoint": 6,
                "chain_id": 1212101,
                "from": "0x2d4c699936bcccbcde1ea12392b080181f17c2c2",
                "nonce": 0,
                "transaction_type": "TokenTransfer",
                "data": {
                    "value": "200000000",
                    "recipient": "0xc7a8e117cb43d7935da4c30b9f9d0cdb5a372808",
                    "token": "0xf864012249f6843fbdc1eb0d55aff9252c09cef8"
                },
                "signature": {
                    "r": "0x197f31a5ab1492c2a06d29a419e49e0d8fc439ce6e61225babf66e6801e76668",
                    "s": "0x41f0a26800b13b7782267af56ebd467220c3d49f02834ff0420124b5aa2ac626",
                    "v": 1
                }
            }
        ],
        "size": 2251
    })
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
