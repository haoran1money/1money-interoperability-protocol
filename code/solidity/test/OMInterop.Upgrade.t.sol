// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

import {OMInterop} from "../src/OMInterop.sol";
import {IOMInterop, InteropProtocol} from "../src/IOMInterop.sol";
import {OMInteropV2} from "./OMInteropV2.sol";

// Minimal UUPS interface for upgrade needed to call upgradeToAndCall in tests
interface IUUPSUpgradeable {
    function upgradeToAndCall(address newImplementation, bytes calldata data) external payable;
}

contract UpgradeTest is Test {
    OMInterop implV1;
    OMInterop om;
    address proxy;

    address internal constant OWNER = address(0xA11CE);
    address internal constant OPERATOR = address(0xB0B);
    address internal constant RELAYER = address(0xC0DE);

    address internal constant OM_TOKEN = address(0x03);
    address internal constant SC_TOKEN = address(0x04);

    function setUp() public {
        // Deploy V1 implementation
        implV1 = new OMInterop();

        bytes32 uuid1 = OMInterop(address(implV1)).proxiableUUID();
        assertTrue(uuid1 != bytes32(0), "implV1 not UUPS");

        // Prepare initializer calldata
        bytes memory init = abi.encodeCall(OMInterop.initialize, (OWNER, OPERATOR, RELAYER));

        // Deploy ERC1967 proxy pointing to V1, run initializer
        ERC1967Proxy p = new ERC1967Proxy(address(implV1), init);
        proxy = address(p);

        // Interact through the proxy using V1 ABI
        om = OMInterop(proxy);

        vm.prank(OPERATOR);
        om.mapTokenAddresses(OM_TOKEN, SC_TOKEN, InteropProtocol.Mock);
    }

    function testUpgradeChangesLogicAndKeepsStorage() public {
        // Verify storage (owner/operator/relayer) pre-upgrade
        assertEq(om.owner(), OWNER, "owner mismatch pre-upgrade");
        assertEq(om.operator(), OPERATOR, "operator mismatch pre-upgrade");
        assertEq(om.relayer(), RELAYER, "relayer mismatch pre-upgrade");
        assertEq(om.version(), "v1.0.0", "version mismatch pre-upgrade");

        {
            // calling proxiableUUID on the *implementation* (not proxy) must not revert
            bytes32 uuid = OMInterop(address(implV1)).proxiableUUID();
            // not strictly required to compare the value; presence is enough
            assertTrue(uuid != bytes32(0), "implV1 not UUPS");
        }

        vm.expectEmit(true, true, false, true, address(om));
        emit IOMInterop.OMInteropSent(0, address(0x1), 2, OM_TOKEN, 1);

        // Verify V1 logic: bridgeTo
        vm.startPrank(RELAYER);

        om.bridgeTo({
            from: address(0x1),
            bbNonce: 0,
            to: address(0x2),
            amount: 10,
            dstChainId: 1,
            escrowFee: 2,
            omToken: OM_TOKEN,
            checkpointId: 3,
            bridgeData: ""
        });

        assertEq(om.getLatestInboundNonce(), 1);

        vm.stopPrank();

        // ---- Upgrade to V2 ----
        OMInteropV2 implV2 = new OMInteropV2();

        vm.prank(OWNER);
        {
            bytes32 uuid2 = OMInterop(address(implV2)).proxiableUUID();
            assertTrue(uuid2 != bytes32(0), "implV2 not UUPS");
        }
        vm.prank(OWNER);
        IUUPSUpgradeable(proxy).upgradeToAndCall(address(implV2), bytes(""));

        vm.expectEmit(true, true, false, true, address(om));
        emit IOMInterop.OMInteropSent(1, address(0x1), 2, OM_TOKEN, 1);

        // Verify V1 logic: bridgeTo
        vm.startPrank(RELAYER);

        om.bridgeTo({
            from: address(0x1),
            bbNonce: 1,
            to: address(0x2),
            amount: 10,
            dstChainId: 1,
            escrowFee: 2,
            omToken: OM_TOKEN,
            checkpointId: 3,
            bridgeData: ""
        });
        assertEq(om.getLatestInboundNonce(), 2);

        assertEq(om.version(), "v2.0.0", "version mismatch post-upgrade");

        vm.stopPrank();

        // Storage MUST be intact
        assertEq(om.owner(), OWNER, "owner mismatch post-upgrade");
        assertEq(om.operator(), OPERATOR, "operator lost across upgrade");
        assertEq(om.relayer(), RELAYER, "relayer mismatch post-upgrade");
    }
}
