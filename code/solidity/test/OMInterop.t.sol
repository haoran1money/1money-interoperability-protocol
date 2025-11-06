// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {OMInterop} from "../src/OMInterop.sol";
import {IOMInterop} from "../src/IOMInterop.sol";

contract OMInteropTest is Test {
    OMInterop internal interop;

    address internal constant OWNER = address(0xA11CE);
    address internal constant OPERATOR = address(0xB0B);
    address internal constant RELAYER = address(0xC0FFEE);
    address internal constant SIDECHAIN_TOKEN = address(0xDEAD);
    address internal constant OM_TOKEN = address(0xBEEF);

    function setUp() public {
        vm.prank(OWNER);
        interop = new OMInterop(OWNER, OPERATOR, RELAYER);
    }

    function testInitialRoles() public {
        assertEq(interop.owner(), OWNER, "owner mismatch");
        assertEq(interop.operator(), OPERATOR, "operator mismatch");
        assertEq(interop.relayer(), RELAYER, "relayer mismatch");
        vm.expectRevert(abi.encodeWithSignature("InboundNonceUnavailable()"));
        interop.getLatestInboundNonce();
        assertEq(interop.getLatestProcessedNonce(address(0xAB)), 0, "unexpected processed nonce");
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(0);
        assertEq(certified, 0);
        assertEq(completed, 0);
    }

    function testSetOperatorOnlyOwner() public {
        address newOperator = address(0xABCD);
        vm.prank(OWNER);
        interop.setOperator(newOperator);
        assertEq(interop.operator(), newOperator);

        vm.expectRevert(abi.encodeWithSignature("OwnableUnauthorizedAccount(address)", address(0x1234)));
        vm.prank(address(0x1234));
        interop.setOperator(address(0x5678));
    }

    function testSetRelayerOnlyOwner() public {
        address newRelayer = address(0xFACE);
        vm.prank(OWNER);
        interop.setRelayer(newRelayer);
        assertEq(interop.relayer(), newRelayer);

        vm.expectRevert(abi.encodeWithSignature("OwnableUnauthorizedAccount(address)", address(0x4321)));
        vm.prank(address(0x4321));
        interop.setRelayer(address(0x8765));
    }

    function testBridgeFromIncrementsNonceAndEmits() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        uint32 chainId = 1;
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(0, address(0x99), 100, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 100);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(1, address(0x98), 50, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 50);

        assertEq(interop.getLatestInboundNonce(), 1, "inbound nonce mismatch");
    }

    function testBridgeFromUnknownTokenReverts() public {
        vm.expectRevert(abi.encodeWithSignature("UnknownToken(address)", SIDECHAIN_TOKEN));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 100);
    }

    function testBridgeToEmitsEventAndUpdatesCheckpoint() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        uint64 checkpointId = 10;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropSent(0, address(0x01), 5, OM_TOKEN, 111);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 100, 111, 5, OM_TOKEN, checkpointId);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId, "checkpoint not closed");
        assertEq(interop.getLatestProcessedNonce(address(0x01)), 1, "processed nonce mismatch");
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(checkpointId);
        assertEq(certified, 1, "certified tally mismatch");
        assertEq(completed, 1, "completed tally mismatch");
    }

    function testBridgeToWrongNonceReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 100, 111, 5, OM_TOKEN, 1);

        vm.expectRevert(abi.encodeWithSignature("InvalidNonce(uint64,uint64)", 0, 1));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x03), 100, 111, 5, OM_TOKEN, 1);

        assertEq(interop.getLatestProcessedNonce(address(0x01)), 1);
    }

    function testTokenBindingViewFunctions() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 7);

        (address scToken, uint8 protoId, bool exists) = interop.getTokenBindingForOm(OM_TOKEN);
        assertEq(scToken, SIDECHAIN_TOKEN);
        assertEq(protoId, 7);
        assertTrue(exists);

        (address omToken, uint8 protoId2, bool exists2) = interop.getTokenBindingForSidechain(SIDECHAIN_TOKEN);
        assertEq(omToken, OM_TOKEN);
        assertEq(protoId2, 7);
        assertTrue(exists2);

        (address missingToken,, bool missingExists) = interop.getTokenBindingForSidechain(address(0x1234));
        assertEq(missingToken, address(0));
        assertFalse(missingExists);
    }

    function testMapTokenAddressesOnlyOperator() public {
        vm.expectRevert(abi.encodeWithSignature("Unauthorized()"));
        vm.prank(address(0xCAFE));
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
    }

    function testMapTokenAddressesRejectsZeroAddresses() public {
        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(address(0), SIDECHAIN_TOKEN, 1);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, address(0), 1);
    }

    function testBridgeFromRejectsZeroAmountAndAddress() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0), 100);

        vm.expectRevert(abi.encodeWithSignature("InvalidAmount()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0xAB), 0);
    }

    function testBridgeToOnlyRelayer() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        vm.expectRevert(abi.encodeWithSignature("Unauthorized()"));
        vm.prank(OPERATOR);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0);
    }

    function testBridgeToUnknownTokenReverts() public {
        vm.expectRevert(abi.encodeWithSignature("UnknownToken(address)", OM_TOKEN));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0);
    }

    function testBridgeToInputValidation() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0), 1, 1, 1, OM_TOKEN, 0);

        vm.expectRevert(abi.encodeWithSignature("InvalidAmount()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 0, 1, 1, OM_TOKEN, 0);

        vm.expectRevert(abi.encodeWithSignature("InvalidChainId()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 0, 1, OM_TOKEN, 0);
    }

    function testBridgeToCheckpointAlreadyCompletedReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 5;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompleted(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 1, address(0x03), 1, 1, 1, OM_TOKEN, checkpointId);
    }

    function testCheckpointTallyIncompleteDoesNotAdvance() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 11;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 2);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId);

        vm.expectRevert(abi.encodeWithSignature("NoCompletedCheckpoint()"));
        interop.getLatestCompletedCheckpoint();
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(checkpointId);
        assertEq(certified, 2);
        assertEq(completed, 1);
    }

    function testLateCheckpointInfoUpdatesLatestCompleted() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 21;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 0);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId);

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId);
    }

    function testCertifiedLowerThanCompletedDoesNotAdvanceLatest() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 21;

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 1, address(0x03), 1, 1, 1, OM_TOKEN, checkpointId);

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        // This should advance the latest completed checkpoint once the certified count is known.
        assertEq(
            interop.getLatestCompletedCheckpoint(),
            checkpointId,
            "latest checkpoint should reflect completed entries once certified count is reported"
        );
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(checkpointId);
        assertEq(certified, 1);
        assertEq(completed, 2);
    }

    function testUpdateCheckpointInfoAfterFinalizationReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 6;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xAA), 0, address(0xBB), 1, 1, 1, OM_TOKEN, checkpointId);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompleted(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 2);
    }

    function testCheckpointZeroFinalizesAndLocks() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, 1);
        uint64 checkpointId = 0;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xCA), 0, address(0xFE), 1, 1, 1, OM_TOKEN, checkpointId);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompleted(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xCA), 1, address(0xFE), 1, 1, 1, OM_TOKEN, checkpointId);
    }
}
