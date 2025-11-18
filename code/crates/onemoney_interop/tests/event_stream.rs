use core::time::Duration;

use alloy_node_bindings::Anvil;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;
use futures::StreamExt;
use onemoney_interop::contract::OMInterop::{self, OMInteropEvents};
use onemoney_interop::error::Error as OMInteropError;
use onemoney_interop::event::{event_stream, OMInteropLog};
use tracing::{debug, info};

async fn next_event<T>(stream: &mut T) -> OMInteropEvents
where
    T: futures::Stream<Item = Result<OMInteropLog, OMInteropError>> + Unpin,
{
    tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("timed out waiting for event")
        .expect("event stream ended unexpectedly")
        .expect("error in event stream")
        .inner
        .data
}

#[tokio::test]
#[test_log::test]
async fn event_stream_captures_ominterop_events() -> color_eyre::Result<()> {
    info!("starting OMInterop event stream integration test");

    let anvil = Anvil::new().try_spawn()?;

    let http_endpoint = anvil.endpoint_url();
    let ws_endpoint = anvil.ws_endpoint_url();
    debug!(%http_endpoint, %ws_endpoint, "anvil endpoints");

    let keys = anvil.keys();
    let addresses = anvil.addresses();

    let owner_wallet: PrivateKeySigner = keys[0].clone().into();
    let operator_wallet: PrivateKeySigner = keys[1].clone().into();
    let relayer_wallet: PrivateKeySigner = keys[2].clone().into();
    let sc_token_wallet: PrivateKeySigner = keys[3].clone().into();
    let new_operator = addresses[4];
    let new_relayer_wallet: PrivateKeySigner = keys[5].clone().into();
    let user_address = addresses[6];
    let om_token = addresses[7];
    let destination = addresses[8];
    let new_owner = addresses[9];

    let owner_addr = owner_wallet.address();
    let operator_addr = operator_wallet.address();
    let relayer_addr = relayer_wallet.address();
    let sc_token_addr = sc_token_wallet.address();
    let new_relayer_addr = new_relayer_wallet.address();

    let owner_provider = ProviderBuilder::new()
        .wallet(owner_wallet)
        .connect_http(http_endpoint.clone());
    let operator_provider = ProviderBuilder::new()
        .wallet(operator_wallet)
        .connect_http(http_endpoint.clone());
    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet)
        .connect_http(http_endpoint.clone());
    let new_relayer_provider = ProviderBuilder::new()
        .wallet(new_relayer_wallet)
        .connect_http(http_endpoint.clone());

    let contract = OMInterop::deploy(
        owner_provider.clone(),
        owner_addr,
        operator_addr,
        relayer_addr,
    )
    .await?;
    let contract_addr = *contract.address();
    info!(?contract_addr, "deployed OMInterop contract");

    let operator_contract = OMInterop::new(contract_addr, operator_provider.clone());
    operator_contract
        .mapTokenAddresses(om_token, sc_token_addr, 1)
        .send()
        .await?
        .get_receipt()
        .await?;
    debug!("mapTokenAddresses transaction confirmed");

    let mut stream = event_stream(http_endpoint, ws_endpoint, contract_addr, 0).await;
    debug!("subscribed to OMInterop event stream");

    let ownership_transferred = next_event(&mut stream).await;
    match ownership_transferred {
        OMInterop::OMInteropEvents::OwnershipTransferred(event) => {
            assert_eq!(event.previousOwner, Address::repeat_byte(0));
            assert_eq!(event.newOwner, owner_addr);
            info!(?owner_addr, "ownership transferred");
        }
        _ => panic!("unexpected ownership event variant"),
    }

    let initial_operator = next_event(&mut stream).await;
    match initial_operator {
        OMInterop::OMInteropEvents::OperatorUpdated(event) => {
            assert_eq!(event.newOperator, operator_addr);
            info!(?operator_addr, "initial operator set");
        }
        _ => panic!("unexpected operator event variant"),
    }

    let initial_relayer = next_event(&mut stream).await;
    match initial_relayer {
        OMInterop::OMInteropEvents::RelayerUpdated(event) => {
            assert_eq!(event.newRelayer, relayer_addr);
            info!(?relayer_addr, "initial relayer set");
        }
        _ => panic!("unexpected relayer event variant"),
    }

    operator_contract
        .setRateLimit(om_token, U256::from(10000), U256::from(3600))
        .send()
        .await?
        .get_receipt()
        .await?;
    debug!("setRateLimit transaction confirmed");

    let updated_rate_limit = next_event(&mut stream).await;
    debug!("received updated_rate_limit event");
    match updated_rate_limit {
        OMInterop::OMInteropEvents::RateLimitsChanged(event) => {
            info!(?event, "rate limit updated");
        }
        ev => panic!("unexpected operator update event: {ev:?}"),
    }

    let owner_contract = OMInterop::new(contract_addr, owner_provider.clone());
    owner_contract
        .setOperator(new_operator)
        .send()
        .await?
        .get_receipt()
        .await?;

    let updated_operator = next_event(&mut stream).await;
    debug!("received operator update event");
    match updated_operator {
        OMInterop::OMInteropEvents::OperatorUpdated(event) => {
            assert_eq!(event.newOperator, new_operator);
            info!(?new_operator, "operator updated");
        }
        _ => panic!("unexpected operator update event"),
    }

    owner_contract
        .setRelayer(new_relayer_addr)
        .send()
        .await?
        .get_receipt()
        .await?;

    let updated_relayer = next_event(&mut stream).await;
    debug!("received relayer update event");
    match updated_relayer {
        OMInterop::OMInteropEvents::RelayerUpdated(event) => {
            assert_eq!(event.newRelayer, new_relayer_addr);
            info!(?new_relayer_addr, "relayer updated");
        }
        _ => panic!("unexpected relayer update event"),
    }

    let sc_token_contract = OMInterop::new(contract_addr, sc_token_provider.clone());
    sc_token_contract
        .bridgeFrom(user_address, U256::from(500u64))
        .send()
        .await?
        .get_receipt()
        .await?;

    let received_event = next_event(&mut stream).await;
    debug!("received OMInteropReceived event");
    match received_event {
        OMInterop::OMInteropEvents::OMInteropReceived(event) => {
            assert_eq!(event.nonce, 0);
            assert_eq!(event.to, user_address);
            assert_eq!(event.amount, U256::from(500u64));
            assert_eq!(event.omToken, om_token);
            assert_eq!(event.srcChainId, 1);
            info!(
                nonce = %event.nonce,
                to = ?user_address,
                amount = ?event.amount,
                om_token = ?event.omToken,
                src_chain_id = event.srcChainId,
                "bridgeFrom emitted"
            );
        }
        _ => panic!("unexpected OMInteropReceived event"),
    }

    let relayer_contract = OMInterop::new(contract_addr, new_relayer_provider.clone());
    relayer_contract
        .bridgeTo(
            user_address,
            0,
            destination,
            U256::from(250u64),
            10,
            U256::from(5u64),
            om_token,
            1,
            Bytes::new(),
        )
        .send()
        .await?
        .get_receipt()
        .await?;

    let sent_event = next_event(&mut stream).await;
    debug!("received OMInteropSent event");
    match sent_event {
        OMInterop::OMInteropEvents::OMInteropSent(event) => {
            assert_eq!(event.nonce, 1);
            assert_eq!(event.from, user_address);
            assert_eq!(event.refundAmount, U256::from(5u64));
            assert_eq!(event.omToken, om_token);
            assert_eq!(event.dstChainId, 10);
            info!(
                nonce = %event.nonce,
                from = ?user_address,
                refund = ?event.refundAmount,
                om_token = ?event.omToken,
                dst_chain_id = event.dstChainId,
                destination = ?destination,
                "bridgeTo emitted"
            );
        }
        _ => panic!("unexpected OMInteropSent event"),
    }

    owner_contract
        .transferOwnership(new_owner)
        .send()
        .await?
        .get_receipt()
        .await?;

    let ownership_event = next_event(&mut stream).await;
    debug!("received OwnershipTransferred event");
    match ownership_event {
        OMInterop::OMInteropEvents::OwnershipTransferred(event) => {
            assert_eq!(event.previousOwner, owner_addr);
            assert_eq!(event.newOwner, new_owner);
            info!(
                previous_owner = ?owner_addr,
                new_owner = ?new_owner,
                "ownership transferred"
            );
        }
        _ => panic!("unexpected OwnershipTransferred event"),
    }

    assert!(
        tokio::time::timeout(Duration::from_millis(200), stream.next())
            .await
            .ok()
            .flatten()
            .is_none(),
        "expected no further events"
    );

    Ok(())
}
