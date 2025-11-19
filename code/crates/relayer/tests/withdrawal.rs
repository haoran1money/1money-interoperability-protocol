pub mod utils;

use core::time::Duration;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::U256;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolEvent;
use color_eyre::eyre::Result;
use onemoney_interop::contract::OMInterop::OMInteropSent;
use onemoney_interop::contract::{OMInterop, TxHashMapping};
use onemoney_protocol::{Client, PaymentPayload};
use relayer::config::Config;
use tracing::info;
use utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};

use crate::utils::account::{fetch_balance, wait_for_eventual_balance};
use crate::utils::setup::{e2e_test_context, E2ETestContext};
use crate::utils::spawn_relayer_and;
use crate::utils::transaction::burn_and_bridge::burn_and_bridge;

const ONE_MONEY_BASE_URL: &str = "http://127.0.0.1:18555";

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn test_withdrawal(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        anvil,
        relayer_wallet,
        sc_token_wallet,
        token_address,
        interop_contract_addr,
        onemoney_client,
        tx_mapping_contract_addr,
        ..
    } = e2e_test_context;

    let keys = anvil.keys();
    let http_endpoint = anvil.endpoint_url();
    let ws_endpoint = anvil.ws_endpoint_url();

    let sc_token_addr = sc_token_wallet.address();
    let relayer_addr = relayer_wallet.address();

    let sender_wallet: PrivateKeySigner = keys[6].clone().into();
    let sender = sender_wallet.address();
    let recipient = anvil.addresses()[7];

    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);

    operator_client
        .mint_token(sender, U256::from(10000000), token_address)
        .await?;

    // TODO: Temporary solution adds tokens to the relayer account until
    // fees are correctly transferred by 1Money
    operator_client
        .mint_token(relayer_addr, U256::from(10000000), token_address)
        .await?;

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_http_url: http_endpoint.clone(),
        side_chain_ws_url: ws_endpoint.clone(),
        interop_contract_address: interop_contract_addr,
        relayer_private_key: relayer_wallet.clone(),
        tx_mapping_contract_address: tx_mapping_contract_addr,
    };

    let relayer_provider = ProviderBuilder::new()
        .wallet(relayer_wallet.clone())
        .connect_http(http_endpoint.clone());
    let relayer_contract = OMInterop::new(interop_contract_addr, relayer_provider.clone());

    spawn_relayer_and(config.clone(), Duration::from_secs(1), || {
        let transfer_amount_1 = U256::from(500u64);
        let transfer_amount_2 = U256::from(400u64);
        let withdrawal_amount_1 = U256::from(10);
        let withdrawal_amount_2 = U256::from(7);

        let fee_amount = U256::from(1);

        let client_url = onemoney_client.base_url().to_string();

        let mapping_contract =
            TxHashMapping::new(tx_mapping_contract_addr, sc_token_provider.clone());
        async move {
            // First deposit
            let sender_balance_before_tx =
                fetch_balance(&onemoney_client, sender, token_address).await?;
            let recipient_balance_before_tx =
                fetch_balance(&onemoney_client, recipient, token_address).await?;

            info!(
                amount = %transfer_amount_1,
                ?sender,
                ?sc_token_addr,
                ?token_address,
                "Invoking payment in 1Money"
            );

            let recent_checkpoint = onemoney_client.get_checkpoint_number().await?.number;
            let chain_id = onemoney_client.fetch_chain_id_from_network().await?;
            let sender_nonce = onemoney_client.get_account_nonce(sender).await?.nonce;

            let payload = PaymentPayload {
                recent_checkpoint,
                chain_id,
                nonce: sender_nonce,
                recipient,
                value: transfer_amount_1,
                token: token_address,
            };

            onemoney_client
                .send_payment(payload, &sender_wallet.to_bytes().encode_hex_with_prefix())
                .await?;

            let sender_balance = wait_for_eventual_balance(
                &onemoney_client,
                sender,
                token_address,
                sender_balance_before_tx - transfer_amount_1,
            )
            .await?;

            let recipient_balance = wait_for_eventual_balance(
                &onemoney_client,
                recipient,
                token_address,
                recipient_balance_before_tx + transfer_amount_1,
            )
            .await?;

            info!(
                ?recipient,
                sender_balance = %sender_balance,
                recipient_balance = %recipient_balance,
                "1Money balance observed after first payment"
            );

            // First withdrawal
            let balance_before_tx = fetch_balance(&onemoney_client, sender, token_address).await?;
            let bbnonce_before_tx = onemoney_client.get_account_bbonce(sender).await?;

            let first_tx_response = burn_and_bridge(
                client_url.clone(),
                sender_wallet.clone(),
                1,
                recipient,
                token_address,
                withdrawal_amount_1,
                fee_amount,
            )
            .await?;

            // Wait for the bb_nonce to be incremented
            let new_bbnonce = tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    let new_bbnonce = onemoney_client.get_account_bbonce(sender).await?;
                    if new_bbnonce.bbnonce > bbnonce_before_tx.bbnonce {
                        break Ok::<_, color_eyre::eyre::Report>(new_bbnonce);
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            // Assert the bridgeTo was processed by verifying that the bbNonce was incremented
            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract
                        .getLatestProcessedNonce(sender)
                        .call()
                        .await?
                        == new_bbnonce.bbnonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract.getLatestInboundNonce().call().await?
                        == onemoney_client.get_account_nonce(relayer_addr).await?.nonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            let target_balance = wait_for_eventual_balance(
                &onemoney_client,
                sender,
                token_address,
                balance_before_tx - withdrawal_amount_1,
            )
            .await?;

            info!(
                ?sender,
                balance = %target_balance,
                "1Money balance observed after first BurnAndBridge"
            );

            // Second withdrawal
            let balance_before_tx = fetch_balance(&onemoney_client, sender, token_address).await?;
            let bbnonce_before_tx = onemoney_client.get_account_bbonce(sender).await?;
            burn_and_bridge(
                client_url,
                sender_wallet.clone(),
                1,
                recipient,
                token_address,
                withdrawal_amount_2,
                fee_amount,
            )
            .await?;

            // Wait for the bb_nonce to be incremented
            let new_bbnonce = tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    let new_bbnonce = onemoney_client.get_account_bbonce(sender).await?;
                    if new_bbnonce.bbnonce > bbnonce_before_tx.bbnonce {
                        break Ok::<_, color_eyre::eyre::Report>(new_bbnonce);
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            // Assert the bridgeTo was processed by verifying that the bbNonce was incremented
            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract
                        .getLatestProcessedNonce(sender)
                        .call()
                        .await?
                        == new_bbnonce.bbnonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract.getLatestInboundNonce().call().await?
                        == onemoney_client.get_account_nonce(relayer_addr).await?.nonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            let target_balance = wait_for_eventual_balance(
                &onemoney_client,
                sender,
                token_address,
                balance_before_tx - withdrawal_amount_2,
            )
            .await?;

            info!(
                ?sender,
                balance = %target_balance,
                "1Money balance observed after second BurnAndBridge"
            );

            // Second deposit
            let sender_balance_before_tx =
                fetch_balance(&onemoney_client, sender, token_address).await?;
            let recipient_balance_before_tx =
                fetch_balance(&onemoney_client, recipient, token_address).await?;

            info!(
                amount = %transfer_amount_2,
                ?sender,
                ?sc_token_addr,
                ?token_address,
                "Invoking payment in 1Money"
            );

            let recent_checkpoint = onemoney_client.get_checkpoint_number().await?.number;
            let chain_id = onemoney_client.fetch_chain_id_from_network().await?;
            let sender_nonce = onemoney_client.get_account_nonce(sender).await?.nonce;

            let payload = PaymentPayload {
                recent_checkpoint,
                chain_id,
                nonce: sender_nonce,
                recipient,
                value: transfer_amount_2,
                token: token_address,
            };

            onemoney_client
                .send_payment(payload, &sender_wallet.to_bytes().encode_hex_with_prefix())
                .await?;

            let sender_balance = wait_for_eventual_balance(
                &onemoney_client,
                sender,
                token_address,
                sender_balance_before_tx - transfer_amount_2,
            )
            .await?;

            let recipient_balance = wait_for_eventual_balance(
                &onemoney_client,
                recipient,
                token_address,
                recipient_balance_before_tx + transfer_amount_2,
            )
            .await?;

            info!(
                ?recipient,
                sender_balance = %sender_balance,
                recipient_balance = %recipient_balance,
                "1Money balance observed after first payment"
            );

            info!("Verifying linked withdrawal transaction hashes");

            let withdrawal_hashes = mapping_contract
                .getWithdrawal(first_tx_response.hash)
                .call()
                .await?;

            assert!(withdrawal_hashes.isSet);

            // Find the transaction receipt linked to the bridgeTo hash
            let bridge_to_transaction = relayer_provider
                .get_transaction_receipt(withdrawal_hashes.bridgeTo)
                .await?
                .expect("Transaction receipt not found");

            // Extract the OMInteropSent event from the transaction logs
            let event = bridge_to_transaction
                .logs()
                .iter()
                .find_map(|log| OMInteropSent::decode_raw_log(log.topics(), &log.data().data).ok())
                .expect("OMInteropReceived not found in tx logs");

            let OMInteropSent {
                from,
                refundAmount,
                omToken,
                ..
            } = event;

            assert_eq!(from, sender);
            assert_eq!(refundAmount, fee_amount);
            assert_eq!(omToken, token_address);

            Ok(())
        }
    })
    .await
}

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn test_clear_withdrawal(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        anvil,
        relayer_wallet,
        sc_token_wallet,
        token_address,
        interop_contract_addr,
        tx_mapping_contract_addr,
        ..
    } = e2e_test_context;

    let keys = anvil.keys();
    let http_endpoint = anvil.endpoint_url();

    let sc_token_addr = sc_token_wallet.address();
    let relayer_addr = relayer_wallet.address();

    let sender_wallet: PrivateKeySigner = keys[6].clone().into();
    let sender = sender_wallet.address();
    let recipient = anvil.addresses()[7];

    let onemoney_client = Client::custom(ONE_MONEY_BASE_URL.to_string())?;
    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);

    operator_client
        .mint_token(sender, U256::from(10000000), token_address)
        .await?;

    // TODO: Temporary solution adds tokens to the relayer account until
    // fees are correctly transferred by 1Money
    operator_client
        .mint_token(relayer_addr, U256::from(10000000), token_address)
        .await?;

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_http_url: http_endpoint.clone(),
        side_chain_ws_url: anvil.ws_endpoint_url(),
        interop_contract_address: interop_contract_addr,
        relayer_private_key: relayer_wallet.clone(),
        tx_mapping_contract_address: tx_mapping_contract_addr,
    };

    let relayer_provider = ProviderBuilder::new()
        .wallet(relayer_wallet.clone())
        .connect_http(http_endpoint.clone());
    let relayer_contract = OMInterop::new(interop_contract_addr, relayer_provider);

    let transfer_amount = U256::from(500u64);
    let withdrawal_amount = U256::from(10);

    let fee_amount = U256::from(1);

    // First deposit
    let sender_balance_before_tx = fetch_balance(&onemoney_client, sender, token_address).await?;
    let recipient_balance_before_tx =
        fetch_balance(&onemoney_client, recipient, token_address).await?;

    info!(
        amount = %transfer_amount,
        ?sender,
        ?sc_token_addr,
        ?token_address,
        "Invoking payment in 1Money"
    );

    let recent_checkpoint = onemoney_client.get_checkpoint_number().await?.number;
    let chain_id = onemoney_client.fetch_chain_id_from_network().await?;
    let sender_nonce = onemoney_client.get_account_nonce(sender).await?.nonce;

    let payload = PaymentPayload {
        recent_checkpoint,
        chain_id,
        nonce: sender_nonce,
        recipient,
        value: transfer_amount,
        token: token_address,
    };

    onemoney_client
        .send_payment(payload, &sender_wallet.to_bytes().encode_hex_with_prefix())
        .await?;

    let sender_balance = wait_for_eventual_balance(
        &onemoney_client,
        sender,
        token_address,
        sender_balance_before_tx - transfer_amount,
    )
    .await?;

    let recipient_balance = wait_for_eventual_balance(
        &onemoney_client,
        recipient,
        token_address,
        recipient_balance_before_tx + transfer_amount,
    )
    .await?;

    info!(
        ?recipient,
        sender_balance = %sender_balance,
        recipient_balance = %recipient_balance,
        "1Money balance observed after first payment"
    );

    // First withdrawal
    let balance_before_tx = fetch_balance(&onemoney_client, sender, token_address).await?;
    let bbnonce_before_tx = onemoney_client.get_account_bbonce(sender).await?;

    burn_and_bridge(
        onemoney_client.base_url().to_string(),
        sender_wallet.clone(),
        1,
        recipient,
        token_address,
        withdrawal_amount,
        fee_amount,
    )
    .await?;

    // Wait a bit to assert balance is unchanged
    tokio::time::sleep(Duration::from_secs(5)).await;

    spawn_relayer_and(config.clone(), Duration::from_secs(1), || {
        async move {
            info!(
                ?sender,
                previous_bbnonce = %bbnonce_before_tx.bbnonce,
                "Will assert bbNonce was correctly updated in 1Money"
            );

            // Wait for the bb_nonce to be incremented
            let new_bbnonce = tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    let new_bbnonce = onemoney_client.get_account_bbonce(sender).await?;
                    if new_bbnonce.bbnonce > bbnonce_before_tx.bbnonce {
                        break Ok::<_, color_eyre::eyre::Report>(new_bbnonce);
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            info!(
                ?sender,
                expected_bbnonce = %new_bbnonce.bbnonce,
                "Will assert bbNonce was correctly updated in Layer1"
            );

            // Assert the bridgeTo was processed by verifying that the bbNonce was incremented
            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract
                        .getLatestProcessedNonce(sender)
                        .call()
                        .await?
                        == new_bbnonce.bbnonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            let expected_balance = balance_before_tx - withdrawal_amount;

            info!(
                ?sender,
                expected_balance = %expected_balance,
                "Will assert sender correctly send the amount and got refunded"
            );

            let target_balance = wait_for_eventual_balance(
                &onemoney_client,
                sender,
                token_address,
                expected_balance,
            )
            .await?;

            tokio::time::timeout(core::time::Duration::from_secs(20), async {
                loop {
                    if relayer_contract.getLatestInboundNonce().call().await?
                        == onemoney_client.get_account_nonce(relayer_addr).await?.nonce
                    {
                        break Ok::<_, color_eyre::eyre::Report>(());
                    }
                    tokio::time::sleep(core::time::Duration::from_secs(1)).await;
                }
            })
            .await??;

            info!(
                ?sender,
                balance = %target_balance,
                "1Money balance observed after first BurnAndBridge"
            );

            Ok(())
        }
    })
    .await
}
