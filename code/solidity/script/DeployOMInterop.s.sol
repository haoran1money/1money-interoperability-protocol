// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Script} from "forge-std/Script.sol";
import {OMInterop} from "../src/OMInterop.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract DeployOMInteropScript is Script {
    function run() external {
        vm.startBroadcast();

        address owner = msg.sender;
        address operator = msg.sender;
        address relayer = msg.sender;

        OMInterop impl = new OMInterop();

        bytes memory initData = abi.encodeCall(OMInterop.initialize, (owner, operator, relayer));

        new ERC1967Proxy(address(impl), initData);

        vm.stopBroadcast();
    }
}
