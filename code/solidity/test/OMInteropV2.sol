// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {OMInterop} from "../src/OMInterop.sol";

// Same storage layout, same inheritance chain.
// We only change logic of quoteEscrow().
contract OMInteropV2 is OMInterop {
    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function version() external pure override returns (string memory) {
        return "v2.0.0";
    }

    // No new storage -> no reinitializer needed.
    // If you add new storage later, use: reinitializer(2) and upgradeToAndCall.
}
