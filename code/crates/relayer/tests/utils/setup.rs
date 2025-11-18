use std::time::{SystemTime, UNIX_EPOCH};

use alloy_node_bindings::{Anvil, AnvilInstance};
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::Result;
use onemoney_interop::contract::{deploy_uups_like, OMInterop};
use onemoney_protocol::{Authority, Client as OnemoneyClient};
use rstest::fixture;

use super::operator::{OperationClient, OPERATOR_PRIVATE_KEY};

pub const ONE_MONEY_BASE_URL: &str = "http://127.0.0.1:18555";

pub struct E2ETestContext {
    pub onemoney_client: OnemoneyClient,
    pub anvil: AnvilInstance,
    pub owner_wallet: PrivateKeySigner,
    pub operator_wallet: PrivateKeySigner,
    pub relayer_wallet: PrivateKeySigner,
    pub sc_token_wallet: PrivateKeySigner,
    pub user_wallet: PrivateKeySigner,
    pub token_address: Address,
    pub contract_addr: Address,
}

#[fixture]
pub async fn e2e_test_context() -> E2ETestContext {
    setup_e2e_test_context()
        .await
        .expect("failed to prepare cross-chain E2E context")
}

async fn setup_e2e_test_context() -> Result<E2ETestContext> {
    let onemoney_client = OnemoneyClient::custom(ONE_MONEY_BASE_URL.to_string())?;

    let anvil = Anvil::new().try_spawn()?;
    let eth_endpoint = anvil.endpoint_url();
    let keys = anvil.keys();

    let owner_wallet: PrivateKeySigner = keys[0].clone().into();
    let relayer_wallet: PrivateKeySigner = keys[1].clone().into();
    let sc_token_wallet: PrivateKeySigner = keys[2].clone().into();
    let user_wallet: PrivateKeySigner = keys[3].clone().into();
    let operator_wallet: PrivateKeySigner = OPERATOR_PRIVATE_KEY.parse()?;

    let operator_address = operator_wallet.address();
    let relayer_address = relayer_wallet.address();

    let operator_client = OperationClient::new(&onemoney_client, OPERATOR_PRIVATE_KEY);

    let token_symbol = format!(
        "OMTST{:x}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );

    let token_address = operator_client
        .issue_new_token(&token_symbol, "Replay Flow Token", 6)
        .await?;
    operator_client
        .grant_authority(Authority::Bridge, relayer_address, token_address, U256::MAX)
        .await?;

    let owner_provider = ProviderBuilder::new()
        .wallet(owner_wallet.clone())
        .connect_http(eth_endpoint.clone());
    let operator_provider = ProviderBuilder::new()
        .wallet(operator_wallet.clone())
        .connect_http(eth_endpoint.clone());

    owner_provider
        .send_transaction(TransactionRequest {
            to: Some(operator_address.into()),
            value: Some(U256::from(5_000_000_000_000_000_000u64)),
            ..Default::default()
        })
        .await?
        .get_receipt()
        .await?;

    let contract = deploy_uups_like(
        &owner_provider,
        owner_wallet.address(),
        operator_address,
        relayer_address,
    )
    .await?
    .1;

    let contract_addr = *contract.address();
    let operator_contract = OMInterop::new(contract_addr, operator_provider);

    operator_contract
        .mapTokenAddresses(token_address, sc_token_wallet.address(), 1)
        .send()
        .await?
        .get_receipt()
        .await?;

    // Set rate limit to 5_000_000_000 tokens per hour
    operator_contract
        .setRateLimit(
            token_address,
            U256::from(5_000_000_000_u64),
            U256::from(3600),
        )
        .send()
        .await?
        .get_receipt()
        .await?;

    Ok(E2ETestContext {
        onemoney_client,
        anvil,
        owner_wallet,
        operator_wallet,
        relayer_wallet,
        sc_token_wallet,
        user_wallet,
        token_address,
        contract_addr,
    })
}
