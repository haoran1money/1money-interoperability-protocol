pub mod utils;

use core::time::Duration;

use alloy_primitives::U256;
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::{eyre, Result};
use onemoney_interop::contract::{OMInterop, TxHashMapping};
use relayer::config::Config;
use relayer::outgoing::stream::relay_outgoing_events;
use tracing::info;
use utils::account::{fetch_balance, wait_for_balance_change};
use utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};
use utils::spawn_relayer_and;

use crate::utils::account::wait_for_eventual_balance;
use crate::utils::setup::{e2e_test_context, E2ETestContext};
use crate::utils::transaction::burn_and_bridge::burn_and_bridge;

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn ominterop_deposit_flow(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        anvil,
        relayer_wallet,
        sc_token_wallet,
        token_address,
        interop_contract_addr,
        tx_mapping_contract_addr,
        onemoney_client,
        ..
    } = e2e_test_context;

    let http_endpoint = anvil.endpoint_url();
    let ws_endpoint = anvil.ws_endpoint_url();

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());

    let one_money_node_url = onemoney_client.base_url();
    let mut one_money_ws_url = onemoney_client.base_url().clone();
    one_money_ws_url.set_scheme("ws").map_err(|_| {
        eyre!(
            "Failed to set `ws` scheme for 1Money URL `{}`",
            onemoney_client.base_url()
        )
    })?;

    let config = Config {
        one_money_node_url: one_money_node_url.clone(),
        one_money_ws_url: one_money_ws_url.clone(),
        side_chain_http_url: http_endpoint.clone(),
        side_chain_ws_url: ws_endpoint.clone(),
        interop_contract_address: interop_contract_addr,
        relayer_private_key: relayer_wallet.clone(),
        tx_mapping_contract_address: tx_mapping_contract_addr,
    };

    spawn_relayer_and(config, || {
        let deposit_amount = U256::from(500u64);
        let recipient = anvil.addresses()[6];
        let sc_token_contract = OMInterop::new(interop_contract_addr, sc_token_provider.clone());
        let mapping_contract =
            TxHashMapping::new(tx_mapping_contract_addr, sc_token_provider.clone());
        async move {
            let initial_balance = fetch_balance(&onemoney_client, recipient, token_address).await?;

            info!(
                amount = %deposit_amount,
                ?recipient,
                sc_token_addr = %sc_token_wallet.address(),
                ?token_address,
                "Invoking bridgeFrom on OMInterop contract"
            );

            let tx_response = sc_token_contract
                .bridgeFrom(recipient, deposit_amount)
                .send()
                .await?
                .get_receipt()
                .await?;

            let target_balance = wait_for_balance_change(
                &onemoney_client,
                recipient,
                token_address,
                initial_balance,
            )
            .await?;

            info!(
                ?recipient,
                balance = %target_balance,
                expected = %deposit_amount,
                "1Money balance observed after bridgeFrom"
            );

            info!("Verifying linked deposit transaction hashes");

            let mint_and_bridge_hash = mapping_contract
                .getDepositByBridgeFrom(tx_response.transaction_hash)
                .call()
                .await?;

            let queried_mint_and_bridge = onemoney_client
                .get_transaction_by_hash(&mint_and_bridge_hash.linked.to_string())
                .await?;

            assert!(matches!(
                queried_mint_and_bridge.data,
                onemoney_protocol::TxPayload::TokenBridgeAndMint { .. }
            ));

            let onemoney_protocol::TxPayload::TokenBridgeAndMint {
                value,
                recipient: mint_recipient,
                token,
                ..
            } = queried_mint_and_bridge.data
            else {
                panic!("Expected TokenBridgeAndMint transaction");
            };

            assert_eq!(value.parse::<U256>()?, deposit_amount);
            assert_eq!(mint_recipient, recipient);
            assert_eq!(token, token_address);

            Ok(())
        }
    })
    .await
}

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn clear_ominterop_deposit(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        onemoney_client,
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

    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());

    let one_money_node_url = onemoney_client.base_url();
    let mut one_money_ws_url = onemoney_client.base_url().clone();
    one_money_ws_url.set_scheme("ws").map_err(|_| {
        eyre!(
            "Failed to set `ws` scheme for 1Money URL `{}`",
            onemoney_client.base_url()
        )
    })?;

    let config = Config {
        one_money_node_url: one_money_node_url.clone(),
        one_money_ws_url: one_money_ws_url.clone(),
        side_chain_http_url: http_endpoint.clone(),
        side_chain_ws_url: anvil.ws_endpoint_url(),
        interop_contract_address: interop_contract_addr,
        relayer_private_key: relayer_wallet.clone(),
        tx_mapping_contract_address: tx_mapping_contract_addr,
    };

    let relayer_nonce = config.sidechain_relayer_nonce().await?;

    let relayer_provider = ProviderBuilder::new()
        .wallet(relayer_wallet.clone())
        .connect_http(http_endpoint.clone());
    let relayer_contract = OMInterop::new(interop_contract_addr, relayer_provider);

    let sender_wallet: PrivateKeySigner = keys[6].clone().into();
    let sender = sender_wallet.address();
    let recipient = anvil.addresses()[7];
    let recipient_withdrawal = anvil.addresses()[8];

    operator_client
        .mint_token(sender, U256::from(10000000), token_address)
        .await?;

    // TODO: Temporary solution adds tokens to the relayer account until
    // fees are correctly transferred by 1Money
    operator_client
        .mint_token(relayer_addr, U256::from(10000000), token_address)
        .await?;

    let deposit_amount = U256::from(500u64);
    let withdrawal_amount = U256::from(400u64);
    let fee_amount = U256::from(1u64);

    let sc_token_contract = OMInterop::new(interop_contract_addr, sc_token_provider.clone());

    let initial_recipient_balance =
        fetch_balance(&onemoney_client, recipient, token_address).await?;
    let initial_sender_balance = fetch_balance(&onemoney_client, sender, token_address).await?;

    let bbnonce_before_tx = onemoney_client.get_account_bbonce(sender).await?;

    info!(
        amount = %deposit_amount,
        ?recipient,
        ?sc_token_addr,
        ?token_address,
        "Invoking bridgeFrom on OMInterop contract"
    );

    // Two bridgeFrom are invoked to advance block number
    sc_token_contract
        .bridgeFrom(recipient, deposit_amount)
        .send()
        .await?
        .get_receipt()
        .await?;

    burn_and_bridge(
        onemoney_client.base_url().to_string(),
        sender_wallet.clone(),
        1,
        recipient_withdrawal,
        token_address,
        withdrawal_amount,
        fee_amount,
    )
    .await?;

    let handler = {
        let config_owned = config.clone();
        tokio::spawn(async move { relay_outgoing_events(&config_owned, relayer_nonce).await })
    };

    // Wait for BurnAndBridge to be processed
    tokio::time::sleep(Duration::from_secs(15)).await;

    handler.abort();

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

    let target_balance = wait_for_eventual_balance(
        &onemoney_client,
        sender,
        token_address,
        initial_sender_balance - withdrawal_amount - fee_amount,
    )
    .await?;

    info!(
        ?sender,
        balance = %target_balance,
        "1Money balance observed after BurnAndBridge but before refund"
    );

    let current_balance = fetch_balance(&onemoney_client, recipient, token_address).await?;
    assert_eq!(current_balance, initial_recipient_balance);

    let expected_balance = initial_recipient_balance + deposit_amount;

    spawn_relayer_and(config.clone(), || async move {
        let target_balance =
            wait_for_eventual_balance(&onemoney_client, recipient, token_address, expected_balance)
                .await?;

        info!(
            ?recipient,
            balance = %target_balance,
            expected = %expected_balance,
            "1Money balance observed after bridgeFrom"
        );

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
            initial_sender_balance - withdrawal_amount,
        )
        .await?;

        info!(
            ?sender,
            balance = %target_balance,
            "1Money balance observed after BurnAndBridge"
        );

        Ok(())
    })
    .await
}
