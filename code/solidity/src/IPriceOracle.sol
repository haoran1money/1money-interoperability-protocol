// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";

interface IPriceOracle is IERC165 {
    function convertNativeTokenToToken(address token, uint256 nativeAmount)
        external
        view
        returns (uint256 price, uint256 lastUpdate);
}
