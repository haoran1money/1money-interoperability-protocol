use alloy_provider::{Provider, ProviderBuilder};
use onemoney_interop::contract::OMInterop::{self, OMInteropErrors};
use onemoney_protocol::Client;

use crate::config::Config;
use crate::incoming::error::Error;

pub async fn get_latest_incomplete_block_number(config: &Config) -> Result<u64, Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_http_url.clone());
    let latest_block_number = provider.get_block_number().await?;

    let contract = OMInterop::new(config.interop_contract_address, provider);
    let client = Client::custom(config.one_money_node_url.to_string())?;

    let om_relayer_nonce = client
        .get_account_nonce(config.relayer_private_key.address())
        .await?
        .nonce;
    let sc_relayer_nonce = sc_inbound_nonce_at(&contract, latest_block_number).await?;

    if om_relayer_nonce > sc_relayer_nonce {
        return Err(Error::Generic(format!("Relayer account nonce is bigger on 1Money side. 1Money {om_relayer_nonce}, Sidechain {sc_relayer_nonce}")));
    }

    // If both nonces are 0 this is a special case and we start relaying from
    // block 0
    if om_relayer_nonce == 0 {
        return Ok(0);
    }

    if om_relayer_nonce == sc_relayer_nonce {
        return Ok(latest_block_number);
    }

    let mut low = 0u64;
    let mut high = latest_block_number;
    while low < high {
        let mid = (low + high).div_ceil(2);
        let nonce_mid = sc_inbound_nonce_at(&contract, mid).await?;

        if nonce_mid <= om_relayer_nonce {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    Ok(low)
}

async fn sc_inbound_nonce_at<P: Provider>(
    contract: &onemoney_interop::contract::OMInterop::OMInteropInstance<P>,
    block: u64,
) -> Result<u64, Error> {
    let res = contract
        .getLatestInboundNonce()
        .call()
        .block(block.into())
        .await;

    match res {
        Ok(n) => Ok(n),
        Err(e) => match e.try_decode_into_interface_error::<OMInteropErrors>() {
            Ok(other) => Err(Error::ContractReverted(other)),
            Err(e) => Err(e.into()),
        },
    }
}
