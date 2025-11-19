// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {OMInterop} from "../src/OMInterop.sol";
import {IOMInterop, InteropProtocol} from "../src/IOMInterop.sol";

contract OMInteropTest is Test {
    OMInterop internal interop;

    address internal constant OWNER = address(0xA11CE);
    address internal constant OPERATOR = address(0xB0B);
    address internal constant RELAYER = address(0xC0FFEE);
    address internal constant SIDECHAIN_TOKEN = address(0xDEAD);
    address internal constant OM_TOKEN = address(0xBEEF);

    bytes32 internal constant BURN_AND_BRIDGE_HASH = keccak256("burnandbridgeTxHash");

    function setUp() public {
        vm.prank(OWNER);

        OMInterop impl = new OMInterop();

        bytes memory initData = abi.encodeCall(OMInterop.initialize, (OWNER, OPERATOR, RELAYER));

        // Deploy proxy
        ERC1967Proxy proxy = new ERC1967Proxy(address(impl), initData);

        // Cast proxy to the OMInterop type
        interop = OMInterop(address(proxy));

        // 10'000 tokens every hour
        vm.prank(OPERATOR);
        interop.setRateLimit(OM_TOKEN, 10_000, 3600);
    }

    function testInitialRoles() public view {
        assertEq(interop.owner(), OWNER, "owner mismatch");
        assertEq(interop.operator(), OPERATOR, "operator mismatch");
        assertEq(interop.relayer(), RELAYER, "relayer mismatch");
        assertEq(interop.getLatestInboundNonce(), 0, "inbound nonce mismatch");
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
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        uint32 chainId = 1;
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(0, address(0x99), 100, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 100);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(1, address(0x98), 50, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 50);

        assertEq(interop.getLatestInboundNonce(), 2, "inbound nonce mismatch");
    }

    function testBridgeFromUnknownTokenReverts() public {
        vm.expectRevert(abi.encodeWithSignature("UnknownToken(address)", SIDECHAIN_TOKEN));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 100);
    }

    function testBridgeToEmitsEventAndUpdatesCheckpoint() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        uint64 checkpointId = 10;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);
        (uint32 certifiedBefore, uint32 completedBefore) = interop.getCheckpointTally(checkpointId);
        assertEq(certifiedBefore, 1);
        assertEq(completedBefore, 0);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropSent(0, address(0x01), 5, OM_TOKEN, 111, BURN_AND_BRIDGE_HASH);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 100, 111, 5, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId, "checkpoint not closed");
        assertEq(interop.getLatestProcessedNonce(address(0x01)), 1, "processed nonce mismatch");
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(checkpointId);
        assertEq(certified, 0, "completed checkpoint should be pruned");
        assertEq(completed, 0, "completed checkpoint should be pruned");
    }

    function testBridgeToWrongNonceReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 100, 111, 5, OM_TOKEN, 1, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("InvalidNonce(uint64,uint64)", 0, 1));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x03), 100, 111, 5, OM_TOKEN, 1, "", BURN_AND_BRIDGE_HASH);

        assertEq(interop.getLatestProcessedNonce(address(0x01)), 1);
    }

    function testBridgeFromAndToSequenceIncrementsInboundNonce() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(0, address(0x1010), 123, OM_TOKEN, 1);
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x1010), 123);
        assertEq(interop.getLatestInboundNonce(), 1, "first inbound nonce incorrect");

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(1, 1);
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropSent(1, address(0xAAAA), 5, OM_TOKEN, 1, BURN_AND_BRIDGE_HASH);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xAAAA), 0, address(0xBBBB), 55, 1, 5, OM_TOKEN, 1, "", BURN_AND_BRIDGE_HASH);
        assertEq(interop.getLatestInboundNonce(), 2, "second inbound nonce incorrect");

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(2, address(0x2020), 456, OM_TOKEN, 1);
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x2020), 456);
        assertEq(interop.getLatestInboundNonce(), 3, "third inbound nonce incorrect");

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(2, 1);
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropSent(3, address(0xCCCC), 5, OM_TOKEN, 1, BURN_AND_BRIDGE_HASH);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xCCCC), 0, address(0xDDDD), 65, 1, 5, OM_TOKEN, 2, "", BURN_AND_BRIDGE_HASH);
        assertEq(interop.getLatestInboundNonce(), 4, "fourth inbound nonce incorrect");
    }

    function testTokenBindingViewFunctions() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        (address scToken, InteropProtocol protoId, bool exists) = interop.getTokenBindingForOm(OM_TOKEN);
        assertEq(scToken, SIDECHAIN_TOKEN);
        assertEq(uint8(protoId), uint8(InteropProtocol.Mock));
        assertTrue(exists);

        (address omToken, InteropProtocol protoId2, bool exists2) = interop.getTokenBindingForSidechain(SIDECHAIN_TOKEN);
        assertEq(omToken, OM_TOKEN);
        assertEq(uint8(protoId2), uint8(InteropProtocol.Mock));
        assertTrue(exists2);

        (address missingToken,, bool missingExists) = interop.getTokenBindingForSidechain(address(0x1234));
        assertEq(missingToken, address(0));
        assertFalse(missingExists);
    }

    function testMapTokenAddressesOnlyOperator() public {
        vm.expectRevert(abi.encodeWithSignature("Unauthorized()"));
        vm.prank(address(0xCAFE));
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
    }

    function testMapTokenAddressesRejectsZeroAddresses() public {
        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(address(0), SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, address(0), InteropProtocol.Mock);
    }

    function testBridgeFromRejectsZeroAmountAndAddress() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0), 100);

        vm.expectRevert(abi.encodeWithSignature("InvalidAmount()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0xAB), 0);
    }

    function testBridgeToOnlyRelayer() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.expectRevert(abi.encodeWithSignature("Unauthorized()"));
        vm.prank(OPERATOR);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);
    }

    function testBridgeToUnknownTokenReverts() public {
        vm.expectRevert(abi.encodeWithSignature("UnknownToken(address)", OM_TOKEN));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);
    }

    function testBridgeToInputValidation() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0), 0, address(0x02), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("InvalidAmount()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 0, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("InvalidChainId()"));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 0, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);
    }

    function testBridgeToCheckpointAlreadyCompletedReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 5;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompletedAndPruned(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 1, address(0x03), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);
    }

    function testCheckpointTallyIncompleteDoesNotAdvance() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 11;
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 2);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("NoCompletedCheckpoint()"));
        interop.getLatestCompletedCheckpoint();
        (uint32 certified, uint32 completed) = interop.getCheckpointTally(checkpointId);
        assertEq(certified, 2);
        assertEq(completed, 1);
    }

    function testLateCheckpointInfoUpdatesLatestCompleted() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 21;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 0);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId);
    }

    function testCertifiedLowerThanCompletedDoesNotAdvanceLatest() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 21;

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 1, address(0x03), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        (uint32 certifiedBefore, uint32 completedBefore) = interop.getCheckpointTally(checkpointId);
        assertEq(certifiedBefore, 0);
        assertEq(completedBefore, 2);

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        // This should advance the latest completed checkpoint once the certified count is known.
        assertEq(
            interop.getLatestCompletedCheckpoint(),
            checkpointId,
            "latest checkpoint should reflect completed entries once certified count is reported"
        );
        (uint32 certifiedAfter, uint32 completedAfter) = interop.getCheckpointTally(checkpointId);
        assertEq(certifiedAfter, 0);
        assertEq(completedAfter, 0);
    }

    function testUpdateCheckpointInfoAfterFinalizationReverts() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 6;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xAA), 0, address(0xBB), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompletedAndPruned(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 2);
    }

    function testCompletingGapAdvancesEarliestCheckpoint() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        // prepare checkpoints 0, 1, 2
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(0, 1);
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(1, 1);
        vm.prank(RELAYER);
        interop.updateCheckpointInfo(2, 1);

        // complete checkpoints 1 and 2 first
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 0, address(0x02), 1, 1, 1, OM_TOKEN, 1, "", BURN_AND_BRIDGE_HASH);

        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 1, address(0x02), 1, 1, 1, OM_TOKEN, 2, "", BURN_AND_BRIDGE_HASH);

        // Checkpoint 0 is still the earliest incomplete until we explicitly finish it.
        assertEq(interop.getLatestCompletedCheckpoint(), 0, "latest checkpoint should still be 0");

        // attempting to complete checkpoint 1 again should revert as it's already completed
        vm.expectRevert(abi.encodeWithSignature("CheckpointCompleted(uint64)", 1));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 2, address(0x02), 1, 1, 1, OM_TOKEN, 1, "", BURN_AND_BRIDGE_HASH);

        // complete checkpoint 0 last, which should trigger the completion of 1 and 2
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 2, address(0x02), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);

        assertEq(interop.getLatestCompletedCheckpoint(), 2, "latest checkpoint should advance to 2");
        vm.expectRevert(abi.encodeWithSignature("CheckpointCompletedAndPruned(uint64)", 0));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0x01), 3, address(0x02), 1, 1, 1, OM_TOKEN, 0, "", BURN_AND_BRIDGE_HASH);
    }

    function testCheckpointZeroFinalizesAndLocks() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);
        uint64 checkpointId = 0;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xCA), 0, address(0xFE), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);

        assertEq(interop.getLatestCompletedCheckpoint(), checkpointId);

        vm.expectRevert(abi.encodeWithSignature("CheckpointCompletedAndPruned(uint64)", checkpointId));
        vm.prank(RELAYER);
        interop.bridgeTo(address(0xCA), 1, address(0xFE), 1, 1, 1, OM_TOKEN, checkpointId, "", BURN_AND_BRIDGE_HASH);
    }

    function testRateLimitExceeded() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        uint32 chainId = 1;

        vm.prank(OPERATOR);
        interop.setRateLimit(OM_TOKEN, 100, 60);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(0, address(0x99), 80, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 80);

        vm.expectRevert(abi.encodeWithSignature("RateLimitExceeded()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 30);

        // simulate waiting 1 minute
        vm.warp(block.timestamp + 360);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(1, address(0x98), 30, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 30);

        assertEq(interop.getLatestInboundNonce(), 2, "inbound nonce mismatch");
    }

    function testDisabledRateLimit() public {
        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, SIDECHAIN_TOKEN, InteropProtocol.Mock);

        uint32 chainId = 1;

        // Step 0 -- set rate limit to 100 tokens per minute
        vm.prank(OPERATOR);
        interop.setRateLimit(OM_TOKEN, 100, 60);

        // Step 1 -- exceed rate limit by trying to bridge 110 tokens within the window
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(0, address(0x99), 80, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 80);

        vm.expectRevert(abi.encodeWithSignature("RateLimitExceeded()"));
        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 30);

        // Step 2 -- disable rate limiting by setting window=0
        vm.prank(OPERATOR);
        interop.setRateLimit(OM_TOKEN, 100, 0);

        // Step 3 -- transfer 110 tokens successfully in two calls
        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(1, address(0x99), 80, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x99), 80);

        vm.expectEmit(true, true, false, true, address(interop));
        emit IOMInterop.OMInteropReceived(2, address(0x98), 30, OM_TOKEN, chainId);

        vm.prank(SIDECHAIN_TOKEN);
        interop.bridgeFrom(address(0x98), 30);

        assertEq(interop.getLatestInboundNonce(), 3, "inbound nonce mismatch");
    }
}
