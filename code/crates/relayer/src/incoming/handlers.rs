use alloy_primitives::{Address, U256};
use onemoney_protocol::client::http::Client;

use crate::incoming::error::Error;

pub async fn handle_om_interop_received(
    _client: &Client,
    _nonce: u64,
    _to: Address,
    _amount: U256,
    _om_token: Address,
) -> Result<(), Error> {
    todo!()
}

pub async fn handle_om_interop_sent(
    _client: &Client,
    _nonce: u64,
    _from: Address,
    _refund_amount: U256,
    _om_token: Address,
) -> Result<(), Error> {
    todo!()
}
