use alloy_sol_types::sol;

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
