mod utils;

use alloy_node_bindings::Anvil;
use alloy_primitives::U256;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::Result;
use onemoney_interop::contract::OMInterop;
use onemoney_protocol::{Authority, Client};
use relayer::config::Config;
use tracing::{debug, info};
use utils::account::{fetch_balance, wait_for_balance_increase};
use utils::operator::{OperationClient, OPERATOR_PRIVATE_KEY};
use utils::spawn_relayer_and;

const ONE_MONEY_BASE_URL: &str = "http://127.0.0.1:18555";

#[tokio::test]
#[test_log::test]
#[ignore = "Requires local Anvil node and 1Money API at http://127.0.0.1:18555"]
async fn ominterop_deposit_flow() -> Result<()> {
    let anvil = Anvil::new().try_spawn()?;
    let http_endpoint = anvil.endpoint_url();
    let ws_endpoint = anvil.ws_endpoint_url();

    debug!(%http_endpoint, %ws_endpoint, "Started Anvil node for testing");

    let keys = anvil.keys();
    let admin_wallet: PrivateKeySigner = keys[0].clone().into();
    let relayer_wallet: PrivateKeySigner = keys[1].clone().into();
    let sc_token_wallet: PrivateKeySigner = keys[2].clone().into();

    let operator_wallet: PrivateKeySigner = OPERATOR_PRIVATE_KEY.parse()?;

    let operator_addr = operator_wallet.address();
    let admin_addr = admin_wallet.address();
    let relayer_addr = relayer_wallet.address();
    let sc_token_addr = sc_token_wallet.address();

    let admin_provider = ProviderBuilder::new()
        .wallet(admin_wallet)
        .connect_http(http_endpoint.clone());
    let operator_provider = ProviderBuilder::new()
        .wallet(operator_wallet)
        .connect_http(http_endpoint.clone());

    admin_provider
        .send_transaction(TransactionRequest {
            to: Some(operator_addr.into()),
            value: Some(U256::from(10_000_000_000_000_000_000_u64)), // 10 ETH
            ..Default::default()
        })
        .await?
        .get_receipt()
        .await?;

    let sc_token_provider = ProviderBuilder::new()
        .wallet(sc_token_wallet.clone())
        .connect_http(http_endpoint.clone());

    let contract = OMInterop::deploy(
        admin_provider.clone(),
        admin_addr,
        operator_addr,
        relayer_addr,
    )
    .await?;
    let contract_addr = *contract.address();
    let operator_contract = OMInterop::new(contract_addr, operator_provider.clone());

    let onemoney_client = Client::custom(ONE_MONEY_BASE_URL.to_string())?;
    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);

    let symbol = format!(
        "OMTST{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
    );
    let name = "Interop Test Token";
    let decimals = 6_u8;

    let one_money_token = operator_client
        .issue_new_token(symbol.as_str(), name, decimals)
        .await?;

    let token_authority_response = operator_client
        .grant_authority(Authority::Bridge, relayer_addr, one_money_token, U256::MAX)
        .await?;

    debug!(
        ?token_authority_response,
        "Granted token Bridge rights to relayer"
    );

    operator_contract
        .mapTokenAddresses(one_money_token, sc_token_addr, 1)
        .send()
        .await?
        .get_receipt()
        .await?;

    let config = Config {
        one_money_node_url: onemoney_client.base_url().clone(),
        side_chain_node_url: http_endpoint.clone(),
        interop_contract_address: contract_addr,
        relayer_private_key: relayer_wallet.clone(),
    };

    spawn_relayer_and(config, || {
        let deposit_amount = U256::from(500u64);
        let recipient = anvil.addresses()[6];
        let sc_token_contract = OMInterop::new(contract_addr, sc_token_provider.clone());
        async move {
            let initial_balance =
                fetch_balance(&onemoney_client, recipient, one_money_token).await?;

            info!(
                amount = %deposit_amount,
                ?recipient,
                ?sc_token_addr,
                ?one_money_token,
                "Invoking bridgeFrom on OMInterop contract"
            );

            sc_token_contract
                .bridgeFrom(recipient, deposit_amount)
                .send()
                .await?
                .get_receipt()
                .await?;

            let target_balance = wait_for_balance_increase(
                &onemoney_client,
                recipient,
                one_money_token,
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
