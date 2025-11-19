use alloy_primitives::{Address, Bytes};
use alloy_provider::Provider;
use alloy_sol_types::{sol, SolCall};

use crate::error::Error;

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc, abi)]
    #[derive(Debug)]
    OMInterop,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../solidity/out/OMInterop.sol/OMInterop.json"
    )
);

sol!(
    #[allow(clippy::too_many_arguments)]
    #[sol(rpc, abi)]
    #[derive(Debug)]
    TxHashMapping,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../solidity/out/TxHashMapping.sol/TxHashMapping.json"
    )
);

sol!(
    #[sol(rpc, abi)]
    #[derive(Debug)]
    ERC1967Proxy,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../solidity/out/ERC1967Proxy.sol/ERC1967Proxy.json"
    )
);

pub async fn deploy_uups_like<P: Provider>(
    provider: &P,
    owner: Address,
    operator: Address,
    relayer: Address,
) -> Result<(Address, OMInterop::OMInteropInstance<&P>), Error> {
    // 1) Deploy the implementation (no constructor args)
    let impl_instance = OMInterop::deploy(provider).await.unwrap();
    let impl_addr = *impl_instance.address();

    // 2) Encode initializer calldata
    let init = OMInterop::initializeCall {
        owner_: owner,
        operator_: operator,
        relayer_: relayer,
    };
    let init_data: Bytes = init.abi_encode().into();

    // 3) Deploy ERC1967Proxy (constructor(address _logic, bytes _data))
    //    NOTE: pass constructor args as a single tuple
    let proxy_instance = ERC1967Proxy::deploy(provider, impl_addr, init_data)
        .await
        .unwrap();
    let proxy_addr = *proxy_instance.address();

    // 4) Bind the implementation ABI at the proxy address
    let om = OMInterop::new(proxy_addr, provider);

    Ok((proxy_addr, om))
}
