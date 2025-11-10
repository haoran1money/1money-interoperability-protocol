use alloy_provider::ProviderBuilder;
use onemoney_interop::contract::OMInterop::{self, OMInteropErrors};

use crate::config::Config;
use crate::outgoing::error::Error;

pub async fn get_earliest_incomplete_checkpoint_number(config: &Config) -> Result<u64, Error> {
    let provider = ProviderBuilder::new()
        .wallet(config.relayer_private_key.clone())
        .connect_http(config.side_chain_node_url.clone());
    let contract = OMInterop::new(config.interop_contract_address, provider);

    let res = contract.getLatestCompletedCheckpoint().call().await;

    match res {
        Ok(n) => Ok(n + 1),
        Err(e) => match e.try_decode_into_interface_error::<OMInteropErrors>() {
            Ok(OMInteropErrors::NoCompletedCheckpoint(_)) => Ok(0),
            Ok(other) => Err(Error::ContractReverted(other)),
            Err(e) => Err(e.into()),
        },
    }
}
