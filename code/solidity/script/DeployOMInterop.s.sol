// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Script} from "forge-std/Script.sol";
import {OMInterop} from "../src/OMInterop.sol";

contract DeployOMInteropScript is Script {
    function run() external {
        vm.startBroadcast();
        new OMInterop(msg.sender, msg.sender, msg.sender);
        vm.stopBroadcast();
    }
}
