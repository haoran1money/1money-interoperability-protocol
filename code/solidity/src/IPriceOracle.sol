// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

interface IPriceOracle {
    function convertNativeTokenToToken(address token, uint256 nativeAmount)
        external
        view
        returns (uint256 price, uint256 lastUpdate);
}
