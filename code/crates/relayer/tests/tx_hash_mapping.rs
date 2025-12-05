pub mod utils;

use core::time::Duration;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::{Bytes, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types_eth::Filter;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolEvent;
use color_eyre::eyre;
use color_eyre::eyre::{eyre, Result};
use onemoney_interop::contract::OMInterop::{OMInteropReceived, OMInteropSent};
use onemoney_interop::contract::{OMInterop, TxHashMapping};
use onemoney_protocol::{PaymentPayload, TokenBridgeAndMintPayload, TxPayload};
use relayer::config::Config;
use tracing::{debug, info};

use crate::utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};
use crate::utils::setup::{e2e_test_context, E2ETestContext};
use crate::utils::spawn_relayer_and;
use crate::utils::transaction::burn_and_bridge::burn_and_bridge;

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn recover_incomplete_deposits(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        anvil,
        relayer_wallet,
        sc_token_wallet,
        interop_contract_addr,
        onemoney_client,
        tx_mapping_contract_addr,
        ..
    } = e2e_test_context;

    let http_endpoint = anvil.endpoint_url();
    let recipient = anvil.addresses()[7];

    let relayer_private_key = relayer_wallet.to_bytes().encode_hex_with_prefix();

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());
    let sc_token_contract = OMInterop::new(interop_contract_addr, sc_token_provider.clone());

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_http_url: http_endpoint.clone(),
        side_chain_ws_url: anvil.ws_endpoint_url(),
        interop_contract_address: interop_contract_addr,
        relayer_private_key: relayer_wallet.clone(),
        tx_mapping_contract_address: tx_mapping_contract_addr,
    };

    let deposit_amount = U256::from(500u64);

    // Send a bridgeFrom transaction and register its hash
    let tx_receipt = sc_token_contract
        .bridgeFrom(recipient, deposit_amount)
        .send()
        .await?
        .get_receipt()
        .await?;

    let mut sidechain_nonce = None;
    let mut to = None;
    let mut amount = None;
    let mut om_token = None;
    let mut src_chain_id = None;

    for log in tx_receipt.logs() {
        if let Ok(ev) = OMInteropReceived::decode_raw_log(log.topics(), &log.data().data) {
            sidechain_nonce = Some(ev.nonce);
            to = Some(ev.to);
            amount = Some(ev.amount);
            om_token = Some(ev.omToken);
            src_chain_id = Some(ev.srcChainId);
            break;
        }
    }

    let sidechain_nonce = sidechain_nonce.ok_or_else(|| eyre::eyre!("missing nonce"))?;
    let to = to.ok_or_else(|| eyre::eyre!("missing to"))?;
    let amount = amount.ok_or_else(|| eyre::eyre!("missing amount"))?;
    let om_token = om_token.ok_or_else(|| eyre::eyre!("missing om_token"))?;
    let src_chain_id = src_chain_id.ok_or_else(|| eyre::eyre!("missing src_chain_id"))?;

    let chain_id = onemoney_client.fetch_chain_id_from_network().await?;

    let first_source_tx_hash = tx_receipt.transaction_hash;

    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider);

    info!(bridgeFromHash = %first_source_tx_hash, "Will register the first deposit transaction hash");

    mapping_contract
        .registerDeposit(first_source_tx_hash)
        .nonce(0)
        .send()
        .await?
        .get_receipt()
        .await?;

    let payload = TokenBridgeAndMintPayload {
        chain_id,
        nonce: sidechain_nonce,
        recipient: to,
        value: amount,
        token: om_token,
        source_chain_id: src_chain_id.into(),
        source_tx_hash: first_source_tx_hash.encode_hex_with_prefix(),
        bridge_metadata: None,
    };

    // Send the BridgeAndMint without linking the hash
    let first_bridge_and_mint_hash = onemoney_client
        .bridge_and_mint(payload, relayer_private_key.as_str())
        .await?;

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Repeat to have 2 incomplete Tx Hash deposits in the TxHashMapping
    let tx_receipt = sc_token_contract
        .bridgeFrom(recipient, deposit_amount)
        .send()
        .await?
        .get_receipt()
        .await?;

    let mut sidechain_nonce = None;
    let mut to = None;
    let mut amount = None;
    let mut om_token = None;
    let mut src_chain_id = None;

    for log in tx_receipt.logs() {
        if let Ok(ev) = OMInteropReceived::decode_raw_log(log.topics(), &log.data().data) {
            sidechain_nonce = Some(ev.nonce);
            to = Some(ev.to);
            amount = Some(ev.amount);
            om_token = Some(ev.omToken);
            src_chain_id = Some(ev.srcChainId);
            break;
        }
    }

    let sidechain_nonce = sidechain_nonce.ok_or_else(|| eyre::eyre!("missing nonce"))?;
    let to = to.ok_or_else(|| eyre::eyre!("missing to"))?;
    let amount = amount.ok_or_else(|| eyre::eyre!("missing amount"))?;
    let om_token = om_token.ok_or_else(|| eyre::eyre!("missing om_token"))?;
    let src_chain_id = src_chain_id.ok_or_else(|| eyre::eyre!("missing src_chain_id"))?;

    let chain_id = onemoney_client.fetch_chain_id_from_network().await?;

    let second_source_tx_hash = tx_receipt.transaction_hash;

    info!(bridgeFromHash = %second_source_tx_hash, "Will register the second deposit transaction hash");

    mapping_contract
        .registerDeposit(second_source_tx_hash)
        .nonce(1)
        .send()
        .await?
        .get_receipt()
        .await?;

    let payload = TokenBridgeAndMintPayload {
        chain_id,
        nonce: sidechain_nonce,
        recipient: to,
        value: amount,
        token: om_token,
        source_chain_id: src_chain_id.into(),
        source_tx_hash: second_source_tx_hash.encode_hex_with_prefix(),
        bridge_metadata: None,
    };

    let second_bridge_and_mint_hash = onemoney_client
        .bridge_and_mint(payload, relayer_private_key.as_str())
        .await?;

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    // Start the relayer and assert the transaction hashes are eventually mapped
    spawn_relayer_and(config.clone(), Duration::from_secs(1), || async move {
        info!("Will assert there are no pending deposits which need to be mapped");
        tokio::time::timeout(core::time::Duration::from_secs(20), async {
            loop {
                let incomplete_hashes = mapping_contract.incompleteDeposits().call().await?;
                if incomplete_hashes.is_empty() {
                    break Ok::<_, color_eyre::eyre::Report>(());
                }
                tokio::time::sleep(core::time::Duration::from_secs(1)).await;
            }
        })
        .await??;

        info!("Will assert the first deposit is correctly mapped");
        tokio::time::timeout(core::time::Duration::from_secs(20), async {
            loop {
                let mapped_hash = mapping_contract
                    .getDepositByBridgeFrom(first_source_tx_hash)
                    .call()
                    .await?;
                if mapped_hash.isSet && mapped_hash.linked == first_bridge_and_mint_hash.hash {
                    break Ok::<_, color_eyre::eyre::Report>(());
                }
                tokio::time::sleep(core::time::Duration::from_secs(1)).await;
            }
        })
        .await??;

        info!("Will assert the second deposit is correctly mapped");
        tokio::time::timeout(core::time::Duration::from_secs(20), async {
            loop {
                let mapped_hash = mapping_contract
                    .getDepositByBridgeFrom(second_source_tx_hash)
                    .call()
                    .await?;
                if mapped_hash.isSet && mapped_hash.linked == second_bridge_and_mint_hash.hash {
                    break Ok::<_, color_eyre::eyre::Report>(());
                }
                tokio::time::sleep(core::time::Duration::from_secs(1)).await;
            }
        })
        .await??;

        Ok(())
    })
    .await
}

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn recover_incomplete_withdrawals(#[future] e2e_test_context: E2ETestContext) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        anvil,
        relayer_wallet,
        sc_token_wallet,
        interop_contract_addr,
        onemoney_client,
        tx_mapping_contract_addr,
        token_address,
        ..
    } = e2e_test_context;

    let keys = anvil.keys();
    let http_endpoint = anvil.endpoint_url();
    let recipient = anvil.addresses()[7];

    let relayer_private_key = relayer_wallet.to_bytes().encode_hex_with_prefix();

    let relayer_addr = relayer_wallet.address();

    let sender_wallet: PrivateKeySigner = keys[6].clone().into();
    let sender = sender_wallet.address();

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());
    let sc_token_contract = OMInterop::new(interop_contract_addr, sc_token_provider.clone());

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

    let withdrawal_amount = U256::from(500u64);
    let fee_amount = U256::from(1);

    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let mapping_contract = TxHashMapping::new(config.tx_mapping_contract_address, provider.clone());
    let relayer_contract = OMInterop::new(interop_contract_addr, provider.clone());

    let burn_and_bridge_hash = burn_and_bridge(
        onemoney_client.base_url().to_string(),
        sender_wallet.clone(),
        1,
        recipient,
        token_address,
        withdrawal_amount,
        fee_amount,
    )
    .await?;

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    let burn_and_bridge_receipt = onemoney_client
        .get_transaction_receipt_by_hash(&burn_and_bridge_hash.hash.to_string())
        .await?;

    let burn_and_bridge_tx = onemoney_client
        .get_transaction_by_hash(&burn_and_bridge_hash.hash.to_string())
        .await?;

    mapping_contract
        .registerWithdrawal(burn_and_bridge_hash.hash)
        .nonce(0)
        .send()
        .await?
        .get_receipt()
        .await?;

    let TxPayload::TokenBurnAndBridge {
        value,
        sender,
        destination_chain_id,
        destination_address,
        escrow_fee,
        bridge_metadata: _,
        token,
    } = burn_and_bridge_tx.data
    else {
        return Err(color_eyre::eyre::eyre!(
            "Expected TokenBurnAndBridge transaction".to_string(),
        ));
    };

    // The bbnonce in the BurnAndBridge receipt is the account's next nonce,
    // so we subtract 1 to get the current nonce.
    let bbnonce = burn_and_bridge_receipt
        .success_info
        .expect("missing `success_info` from BurnAndBridge receipt")
        .bridge_info
        .expect("missing `bridge_info` from BurnAndBridge receipt")
        .bbnonce
        - 1;

    // TODO: Handle bridgeData when it is added to the TokenBurnAndBridge.
    // For now, we pass an empty bytes array.
    let bridge_data = Bytes::new();

    let checkpoint_number = burn_and_bridge_receipt
        .checkpoint_number
        .expect("missing `checkpoint_number` from BurnAndBridge receipt");

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    let tx_receipt = relayer_contract
        .bridgeTo(
            sender,
            bbnonce,
            destination_address.parse()?,
            value.parse()?,
            destination_chain_id.try_into()?,
            escrow_fee.parse()?,
            token,
            checkpoint_number,
            bridge_data,
            burn_and_bridge_tx.hash,
        )
        .nonce(1)
        .send()
        .await
        .map(Ok)
        .or_else(|e| {
            e.try_decode_into_interface_error::<OMInterop::OMInteropErrors>()
                .map(Err)
        })?
        .map_err(|e| eyre!("Failed to send bridgeTo: {e:?}"))?
        .get_receipt()
        .await?;

    debug!(?tx_receipt, "Tx receipt for bridge to");

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    let from = burn_and_bridge_receipt.from;

    let filter = Filter::new()
        .address(*sc_token_contract.address())
        .from_block(0)
        .to_block(100)
        .topic1(from);

    let logs = provider.get_logs(&filter).await?;

    let mut sidechain_nonce = None;
    let mut from = None;
    let mut refund_amount = None;
    let mut om_token = None;

    for log in logs {
        if let Ok(parsed) = OMInteropSent::decode_raw_log(log.topics(), &log.data().data) {
            if parsed.sourceHash != burn_and_bridge_receipt.transaction_hash {
                continue;
            }

            sidechain_nonce = Some(parsed.nonce);
            from = Some(parsed.from);
            refund_amount = Some(parsed.refundAmount);
            om_token = Some(parsed.omToken);
        }
    }
    let sidechain_nonce = sidechain_nonce.ok_or_else(|| eyre::eyre!("missing sidechain_nonce"))?;
    let from = from.ok_or_else(|| eyre::eyre!("missing from"))?;
    let refund_amount = refund_amount.ok_or_else(|| eyre::eyre!("missing refund_amount"))?;
    let om_token = om_token.ok_or_else(|| eyre::eyre!("missing om_token"))?;

    let chain_id = onemoney_client.fetch_chain_id_from_network().await?;

    let payload = PaymentPayload {
        chain_id,
        nonce: sidechain_nonce,
        recipient: from,
        value: refund_amount,
        token: om_token,
    };

    let tx_response = onemoney_client
        .send_payment(payload, &relayer_private_key)
        .await?;

    debug!(?tx_response.hash, "Tx receipt for refund");

    let burn_and_bridge_hash = burn_and_bridge(
        onemoney_client.base_url().to_string(),
        sender_wallet.clone(),
        1,
        recipient,
        token_address,
        withdrawal_amount,
        fee_amount,
    )
    .await?;

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    let burn_and_bridge_receipt = onemoney_client
        .get_transaction_receipt_by_hash(&burn_and_bridge_hash.hash.to_string())
        .await?;

    let burn_and_bridge_tx = onemoney_client
        .get_transaction_by_hash(&burn_and_bridge_hash.hash.to_string())
        .await?;

    mapping_contract
        .registerWithdrawal(burn_and_bridge_hash.hash)
        .nonce(2)
        .send()
        .await?
        .get_receipt()
        .await?;

    let TxPayload::TokenBurnAndBridge {
        value,
        sender,
        destination_chain_id,
        destination_address,
        escrow_fee,
        bridge_metadata: _,
        token,
    } = burn_and_bridge_tx.data
    else {
        return Err(color_eyre::eyre::eyre!(
            "Expected TokenBurnAndBridge transaction".to_string(),
        ));
    };

    // The bbnonce in the BurnAndBridge receipt is the account's next nonce,
    // so we subtract 1 to get the current nonce.
    let bbnonce = burn_and_bridge_receipt
        .success_info
        .expect("missing `success_info` from BurnAndBridge receipt")
        .bridge_info
        .expect("missing `bridge_info` from BurnAndBridge receipt")
        .bbnonce
        - 1;

    // TODO: Handle bridgeData when it is added to the TokenBurnAndBridge.
    // For now, we pass an empty bytes array.
    let bridge_data = Bytes::new();

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;
    let checkpoint_number = burn_and_bridge_receipt
        .checkpoint_number
        .expect("missing `checkpoint_number` from BurnAndBridge receipt");

    let tx_receipt = relayer_contract
        .bridgeTo(
            sender,
            bbnonce,
            destination_address.parse()?,
            value.parse()?,
            destination_chain_id.try_into()?,
            escrow_fee.parse()?,
            token,
            checkpoint_number,
            bridge_data,
            burn_and_bridge_tx.hash,
        )
        .nonce(3)
        .send()
        .await
        .map(Ok)
        .or_else(|e| {
            e.try_decode_into_interface_error::<OMInterop::OMInteropErrors>()
                .map(Err)
        })?
        .map_err(|e| eyre!("Failed to send bridgeTo: {e:?}"))?
        .get_receipt()
        .await?;

    debug!(?tx_receipt, "Tx receipt for bridge to");

    // Wait for the second transaction to be processed
    tokio::time::sleep(core::time::Duration::from_secs(5)).await;

    let incomplete_withdrawal_hashes_before_clearing =
        mapping_contract.incompleteWithdrawals().call().await?;

    assert_eq!(
        incomplete_withdrawal_hashes_before_clearing.len(),
        2,
        "Expected 2 incomplete withdrawal hash mappings"
    );

    let incomplete_refund_hashes_before_clearing =
        mapping_contract.incompleteRefunds().call().await?;

    assert_eq!(
        incomplete_refund_hashes_before_clearing.len(),
        2,
        "Expected 2 incomplete refund hash mappings"
    );

    spawn_relayer_and(config.clone(), Duration::from_secs(1), || async move {
        info!("Will assert there are no pending withdrawals which need to be mapped");
        tokio::time::timeout(core::time::Duration::from_secs(20), async {
            loop {
                let incomplete_hashes = mapping_contract.incompleteWithdrawals().call().await?;
                if incomplete_hashes.is_empty() {
                    break Ok::<_, color_eyre::eyre::Report>(());
                }
                tokio::time::sleep(core::time::Duration::from_secs(1)).await;
            }
        })
        .await??;
        info!("Will assert there are no pending refunds which need to be mapped");
        tokio::time::timeout(core::time::Duration::from_secs(20), async {
            loop {
                let incomplete_hashes = mapping_contract.incompleteRefunds().call().await?;
                if incomplete_hashes.is_empty() {
                    break Ok::<_, color_eyre::eyre::Report>(());
                }
                tokio::time::sleep(core::time::Duration::from_secs(1)).await;
            }
        })
        .await??;

        Ok(())
    })
    .await
}
