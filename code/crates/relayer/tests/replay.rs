#[allow(dead_code)]
mod utils;

use core::future::Future;

use alloy_primitives::hex::ToHexExt;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::ProviderBuilder;
use color_eyre::eyre::Result;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::{
    Error as OnemoneyError, PaymentPayload, TokenBridgeAndMintPayload, TokenBurnAndBridgePayload,
};
use tracing::{debug, info};
use utils::account::{fetch_balance, wait_for_balance_change};
use utils::setup::{e2e_test_context, E2ETestContext};
use utils::transaction::wait_for_transaction;

use crate::utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};

#[rstest::rstest]
#[tokio::test]
#[test_log::test]
#[ignore = "Requires 1Money API at http://127.0.0.1:18555"]
async fn cross_chain_replay_flow_is_enforced(
    #[future] e2e_test_context: E2ETestContext,
) -> Result<()> {
    let e2e_test_context = e2e_test_context.await;
    let E2ETestContext {
        onemoney_client,
        anvil,
        relayer_wallet,
        sc_token_wallet,
        user_wallet,
        token_address,
        interop_contract_addr,
        ..
    } = e2e_test_context;

    let eth_endpoint = anvil.endpoint_url();

    let relayer_address = relayer_wallet.address();
    let user_address = user_wallet.address();
    let relayer_private_key = relayer_wallet.to_bytes().encode_hex_with_prefix();
    let user_private_key = user_wallet.to_bytes().encode_hex_with_prefix();

    let chain_id = onemoney_client.fetch_chain_id_from_network().await?;
    let bridge_amount = U256::from(750u64);
    let refund_amount = U256::from(5u64);
    let net_bridge_amount = bridge_amount
        .checked_add(refund_amount)
        .expect("bridge amount + escrow should not overflow");

    // TODO: Temporary solution adds tokens to the relayer account until
    // fees are correctly transferred by 1Money
    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);
    operator_client
        .mint_token(relayer_address, U256::from(10000000), token_address)
        .await?;

    info!("Step 0: initiate bridgeFrom on the side-chain");
    // --- Step 0: bridgeFrom to create source transaction -------------------------------------------------------------
    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(eth_endpoint.clone());
    let source_tx_hash = OMInterop::new(interop_contract_addr, sc_token_provider)
        .bridgeFrom(user_address, net_bridge_amount)
        .send()
        .await?
        .get_receipt()
        .await?
        .transaction_hash;

    let relayer_provider = ProviderBuilder::new()
        .wallet(relayer_wallet.clone())
        .connect_http(eth_endpoint.clone());
    let relayer_contract = OMInterop::new(interop_contract_addr, relayer_provider);

    info!("Step 1: begin bridge_and_mint replay validation");
    // --- Step 1: bridge_and_mint replay ------------------------------------------------------------------------------
    let initial_user_balance = fetch_balance(&onemoney_client, user_address, token_address).await?;
    debug!(
        %initial_user_balance,
        "Initial user token balance before bridge_and_mint"
    );

    let nonce = onemoney_client
        .get_account_nonce(relayer_address)
        .await?
        .nonce;
    let bridge_payload = TokenBridgeAndMintPayload {
        chain_id,
        nonce,
        recipient: user_address,
        value: net_bridge_amount,
        token: token_address,
        source_chain_id: 1,
        source_tx_hash: source_tx_hash.encode_hex_with_prefix(),
        bridge_metadata: None,
    };

    let first_mint = onemoney_client
        .bridge_and_mint(bridge_payload.clone(), relayer_private_key.as_str())
        .await?;
    assert!(
        wait_for_transaction(&onemoney_client, &first_mint.hash, "bridge_and_mint")
            .await?
            .is_break(),
        "bridge_and_mint was rejected by 1Money"
    );

    let minted_balance = wait_for_balance_change(
        &onemoney_client,
        user_address,
        token_address,
        initial_user_balance,
    )
    .await?;

    debug!(
        %minted_balance,
        "User token balance after bridge_and_mint"
    );

    let minted_delta = minted_balance
        .checked_sub(initial_user_balance)
        .expect("minted balance should never be below the initial balance");
    assert_eq!(
        minted_delta, net_bridge_amount,
        "user balance delta mismatch"
    );
    expect_onemoney_replay(
        onemoney_client.bridge_and_mint(bridge_payload.clone(), relayer_private_key.as_str()),
    )
    .await;

    let post_replay_balance = fetch_balance(&onemoney_client, user_address, token_address).await?;
    assert_eq!(
        post_replay_balance, minted_balance,
        "user balance changed after replay"
    );

    info!("Step 2b: begin burn_and_bridge replay validation");
    // --- Step 2b: burn_and_bridge replay -----------------------------------------------------------------------------
    let pre_burn_balance = fetch_balance(&onemoney_client, user_address, token_address).await?;
    let nonce = onemoney_client.get_account_nonce(user_address).await?.nonce;
    let destination_address = Address::repeat_byte(0xAB);
    let burn_payload = TokenBurnAndBridgePayload {
        chain_id,
        nonce,
        sender: user_address,
        value: bridge_amount,
        token: token_address,
        destination_chain_id: 1,
        destination_address: destination_address.encode_hex_with_prefix(),
        escrow_fee: refund_amount,
        bridge_metadata: None,
        bridge_param: None,
    };

    let first_burn = onemoney_client
        .burn_and_bridge(burn_payload.clone(), user_private_key.as_str())
        .await?;
    assert!(
        wait_for_transaction(&onemoney_client, &first_burn.hash, "burn_and_bridge")
            .await?
            .is_break(),
        "burn_and_bridge was rejected by 1Money"
    );

    let post_burn_balance = wait_for_balance_change(
        &onemoney_client,
        user_address,
        token_address,
        pre_burn_balance,
    )
    .await?;

    debug!(
        %post_burn_balance,
        "User token balance after burn_and_bridge"
    );

    let burn_delta = pre_burn_balance
        .checked_sub(post_burn_balance)
        .expect("burn burned more than available balance");

    assert_eq!(
        burn_delta,
        bridge_amount + refund_amount,
        "user balance did not decrease by bridge_amount"
    );

    expect_onemoney_replay(
        onemoney_client.burn_and_bridge(burn_payload.clone(), user_private_key.as_str()),
    )
    .await;

    let user_balance_after_replay =
        fetch_balance(&onemoney_client, user_address, token_address).await?;
    assert_eq!(
        user_balance_after_replay, post_burn_balance,
        "user balance changed after burn replay"
    );

    info!("Step 3: validate bridgeTo replay protection on side-chain");
    // --- Step 3: side-chain bridgeTo replay --------------------------------------------------------------------------
    let checkpoint_id = 42u64;
    let bb_nonce = 0u64;
    let destination = Address::repeat_byte(0xCD);

    relayer_contract
        .bridgeTo(
            user_address,
            bb_nonce,
            destination,
            bridge_amount,
            1,
            refund_amount,
            token_address,
            checkpoint_id,
            Bytes::new(),
            first_burn.hash,
        )
        .send()
        .await?
        .get_receipt()
        .await?;

    let replay_attempt = relayer_contract
        .bridgeTo(
            user_address,
            bb_nonce,
            destination,
            bridge_amount,
            1,
            refund_amount,
            token_address,
            checkpoint_id,
            Bytes::new(),
            first_burn.hash,
        )
        .send()
        .await;

    let err = replay_attempt.expect_err("expected duplicate bridgeTo to fail");
    let replay_err = err
        .try_decode_into_interface_error::<OMInterop::OMInteropErrors>()
        .expect("expected OMInterop revert");
    match replay_err {
        OMInterop::OMInteropErrors::InvalidNonce(invalid_nonce) => {
            assert_eq!(invalid_nonce.provided, bb_nonce);
            assert_eq!(invalid_nonce.expected, bb_nonce + 1);
        }
        other => panic!("unexpected OMInterop error: {other:?}"),
    }

    info!("Step 4: ensure refund payment replay is rejected");
    // --- Step 4: refund payment replay -------------------------------------------------------------------------------
    let relayer_balance_before_refund =
        fetch_balance(&onemoney_client, relayer_address, token_address).await?;
    let user_balance_before_refund =
        fetch_balance(&onemoney_client, user_address, token_address).await?;

    let nonce = onemoney_client
        .get_account_nonce(relayer_address)
        .await?
        .nonce;

    let payment_payload = PaymentPayload {
        chain_id,
        nonce,
        recipient: user_address,
        value: refund_amount,
        token: token_address,
    };

    let first_refund = onemoney_client
        .send_payment(payment_payload.clone(), relayer_private_key.as_str())
        .await?;
    assert!(
        wait_for_transaction(&onemoney_client, &first_refund.hash, "refund payment")
            .await?
            .is_break(),
        "refund payment was rejected by 1Money"
    );
    let relayer_balance_after_refund = wait_for_balance_change(
        &onemoney_client,
        relayer_address,
        token_address,
        relayer_balance_before_refund,
    )
    .await?;
    let relayer_delta = relayer_balance_before_refund
        .checked_sub(relayer_balance_after_refund)
        .expect("relayer balance increased after refund payment");
    assert_eq!(
        relayer_delta, refund_amount,
        "relayer balance delta mismatch after refund payment"
    );
    let user_balance_after_refund = wait_for_balance_change(
        &onemoney_client,
        user_address,
        token_address,
        user_balance_before_refund,
    )
    .await?;
    let user_delta = user_balance_after_refund
        .checked_sub(user_balance_before_refund)
        .expect("user balance decreased after refund payment");
    assert_eq!(
        user_delta, refund_amount,
        "user balance delta mismatch after refund payment"
    );
    expect_onemoney_replay(
        onemoney_client.send_payment(payment_payload.clone(), relayer_private_key.as_str()),
    )
    .await;
    let relayer_balance_after_replay =
        fetch_balance(&onemoney_client, relayer_address, token_address).await?;
    assert_eq!(
        relayer_balance_after_replay, relayer_balance_after_refund,
        "relayer balance changed after refund replay attempt"
    );
    let user_balance_after_replay =
        fetch_balance(&onemoney_client, user_address, token_address).await?;
    assert_eq!(
        user_balance_after_replay, user_balance_after_refund,
        "user balance changed after refund replay attempt"
    );

    Ok(())
}

async fn expect_onemoney_replay<Fut, T>(fut: Fut)
where
    Fut: Future<Output = core::result::Result<T, OnemoneyError>>,
    T: core::fmt::Debug,
{
    let err = fut
        .await
        .expect_err("expected replay-protected call to fail");
    match err {
        OnemoneyError::BusinessLogic { .. } | OnemoneyError::Api { .. } => {
            debug!(?err, "Observed expected replay rejection from 1Money");
        }
        other => panic!("unexpected error variant when replaying transaction: {other:?}"),
    }
}
