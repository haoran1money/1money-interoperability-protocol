use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use url::Url;

#[derive(clap::Args, Clone)]
pub struct Config {
    /// URL of the 1Money node to connect to
    #[arg(long, env = "OM_NODE_URL", default_value = "http://127.0.0.1:18555")]
    pub one_money_node_url: Url,
    /// URL of the sidechain node to connect to
    #[arg(long, env = "SC_NODE_URL", default_value = "http://127.0.0.1:8545")]
    pub side_chain_node_url: Url,
    /// Address of the interop contract
    #[arg(long, env = "INTEROP_CONTRACT_ADDRESS")]
    pub interop_contract_address: Address,
    /// Private key of the relayer account
    #[arg(long, env = "RELAYER_PRIVATE_KEY")]
    pub relayer_private_key: PrivateKeySigner,
}
