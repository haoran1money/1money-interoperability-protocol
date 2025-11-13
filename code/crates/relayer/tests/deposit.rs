pub mod utils;

use core::time::Duration;

use alloy_primitives::U256;
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::Result;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::Client;
use relayer::config::Config;
use relayer::outgoing::stream::relay_outgoing_events;
use tracing::info;
use utils::account::{fetch_balance, wait_for_balance_change};
use utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};
use utils::spawn_relayer_and;

use crate::utils::account::wait_for_eventual_balance;
use crate::utils::setup::{e2e_test_context, E2ETestContext};
use crate::utils::transaction::burn_and_bridge::burn_and_bridge;

const ONE_MONEY_BASE_URL: &str = "http://127.0.0.1:18555";

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
        contract_addr,
        ..
    } = e2e_test_context;

    let http_endpoint = anvil.endpoint_url();

    let onemoney_client = Client::custom(ONE_MONEY_BASE_URL.to_string())?;

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_node_url: http_endpoint.clone(),
        interop_contract_address: contract_addr,
        relayer_private_key: relayer_wallet.clone(),
    };

    spawn_relayer_and(config, Duration::from_secs(1), || {
        let deposit_amount = U256::from(500u64);
        let recipient = anvil.addresses()[6];
        let sc_token_contract = OMInterop::new(contract_addr, sc_token_provider.clone());
        async move {
            let initial_balance = fetch_balance(&onemoney_client, recipient, token_address).await?;

            info!(
                amount = %deposit_amount,
                ?recipient,
                sc_token_addr = %sc_token_wallet.address(),
                ?token_address,
                "Invoking bridgeFrom on OMInterop contract"
            );

            sc_token_contract
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
        contract_addr,
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

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_node_url: http_endpoint.clone(),
        interop_contract_address: contract_addr,
        relayer_private_key: relayer_wallet.clone(),
    };

    let relayer_nonce = config.sidechain_relayer_nonce().await?;

    let relayer_provider = ProviderBuilder::new()
        .wallet(relayer_wallet.clone())
        .connect_http(http_endpoint.clone());
    let relayer_contract = OMInterop::new(contract_addr, relayer_provider);

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

    let sc_token_contract = OMInterop::new(contract_addr, sc_token_provider.clone());

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
        tokio::spawn(async move {
            relay_outgoing_events(&config_owned, relayer_nonce, 0, Duration::from_secs(1)).await
        })
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
        initial_sender_balance - withdrawal_amount,
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

    spawn_relayer_and(config.clone(), Duration::from_secs(1), || async move {
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
            initial_sender_balance - withdrawal_amount + fee_amount,
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
