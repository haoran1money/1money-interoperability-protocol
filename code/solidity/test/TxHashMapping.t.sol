// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {TxHashMapping} from "../src/TxHashMapping.sol";

contract TxHashMappingTest is Test {
    TxHashMapping map;

    address owner = address(0xA11CE);
    address relayer = address(0xBEEF);
    address stranger = address(0xCAFE);

    bytes32 d1 = keccak256("d1");
    bytes32 d2 = keccak256("d2");
    bytes32 d3 = keccak256("d3");

    bytes32 w1 = keccak256("w1");
    bytes32 w2 = keccak256("w2");
    bytes32 w3 = keccak256("w3");

    bytes32 l1 = keccak256("l1");
    bytes32 l2 = keccak256("l2");
    bytes32 l3 = keccak256("l3");
    bytes32 l4 = keccak256("l4");

    function setUp() public {
        vm.prank(owner);
        map = new TxHashMapping(owner, relayer);
    }

    // ---------- Modifiers & access ----------

    function testRegisterOnlyRelayer() public {
        vm.prank(stranger);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.registerDeposit(d1);

        vm.prank(stranger);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.registerWithdrawal(w1);
    }

    function testRegisterZeroHashReverts() public {
        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.InvalidHash.selector);
        map.registerDeposit(bytes32(0));

        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.InvalidHash.selector);
        map.registerWithdrawal(bytes32(0));
    }

    function testLinkOnlyRelayer() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);
        map.registerDeposit(w1);
        vm.stopPrank();

        vm.startPrank(stranger);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.linkDepositHashes(d1, l1);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.linkWithdrawalHashes(w1, l1);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.linkRefundHashes(w1, l2);
        vm.stopPrank();
    }

    function testLinkZeroHashReverts() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);
        map.registerWithdrawal(w1);
        vm.expectRevert(TxHashMapping.InvalidHash.selector);
        map.linkDepositHashes(d1, bytes32(0));
        vm.expectRevert(TxHashMapping.InvalidHash.selector);
        map.linkWithdrawalHashes(w1, bytes32(0));
        vm.expectRevert(TxHashMapping.InvalidHash.selector);
        map.linkRefundHashes(w1, bytes32(0));
        vm.stopPrank();
    }

    function testSetRelayerOnlyOwner() public {
        vm.prank(stranger);
        vm.expectRevert();
        map.setRelayer(stranger);
    }

    function testSetRelayerZeroAddrReverts() public {
        vm.prank(owner);
        vm.expectRevert(TxHashMapping.InvalidAddress.selector);
        map.setRelayer(address(0));
    }

    function testSetRelayerEmitsAndTakesEffect() public {
        vm.prank(owner);
        vm.expectEmit(true, true, false, true);
        emit TxHashMapping.RelayerUpdated(relayer, stranger);
        map.setRelayer(stranger);

        // old relayer can no longer act
        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.Unauthorized.selector);
        map.registerDeposit(d1);

        // new relayer works
        vm.prank(stranger);
        map.registerDeposit(d1);
    }

    // ---------- Core behavior ----------

    function testRegisterEmitsAndTracksDeposit() public {
        vm.startPrank(relayer);

        vm.expectEmit(true, false, false, true);
        emit TxHashMapping.DepositRegistered(d1);
        map.registerDeposit(d1);

        assertTrue(map.depositExists(d1));
        assertEq(map.getLinkedDeposit(d1), bytes32(0));

        bytes32[] memory arr = map.incompleteDeposits();
        assertEq(arr.length, 1);
        assertEq(arr[0], d1);

        // re-register should revert
        vm.expectRevert(TxHashMapping.AlreadySet.selector);
        map.registerDeposit(d1);

        vm.stopPrank();
    }

    function testRegisterEmitsAndTracksWithdrawal() public {
        vm.startPrank(relayer);

        vm.expectEmit(true, false, false, true);
        emit TxHashMapping.WithdrawalRegistered(w1);
        map.registerWithdrawal(w1);

        assertTrue(map.withdrawalExists(w1));
        assertEq(map.getLinkedWithdrawal(w1), bytes32(0));
        assertEq(map.getLinkedRefund(w1), bytes32(0));

        bytes32[] memory withdrawalArr = map.incompleteWithdrawals();
        assertEq(withdrawalArr.length, 1);
        assertEq(withdrawalArr[0], w1);

        bytes32[] memory refundArr = map.incompleteRefunds();
        assertEq(refundArr.length, 1);
        assertEq(refundArr[0], w1);

        // re-register should revert
        vm.expectRevert(TxHashMapping.AlreadySet.selector);
        map.registerWithdrawal(w1);

        vm.stopPrank();
    }

    function testDepositLinkHappyPathRemovesFromIncomplete() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);

        vm.expectEmit(true, true, false, true);
        emit TxHashMapping.DepositHashesLinked(d1, l1);
        map.linkDepositHashes(d1, l1);
        vm.stopPrank();

        assertEq(map.getLinkedDeposit(d1), l1);

        bytes32[] memory arr = map.incompleteDeposits();
        assertEq(arr.length, 0);

        // Verify reverse mapping
        bytes32 queriedBridgeFrom = map.getDepositByBridgeAndMint(l1);
        assertEq(queriedBridgeFrom, d1);
    }

    function testWithdrawalLinkHappyPathRemovesFromIncomplete() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);

        vm.expectEmit(true, true, false, true);
        emit TxHashMapping.WithdrawalHashesLinked(w1, l1);
        map.linkWithdrawalHashes(w1, l1);

        emit TxHashMapping.RefundHashesLinked(w1, l1);
        map.linkRefundHashes(w1, l2);

        vm.stopPrank();

        assertEq(map.getLinkedWithdrawal(w1), l1);
        assertEq(map.getLinkedRefund(w1), l2);

        bytes32[] memory withdrawalArr = map.incompleteWithdrawals();
        assertEq(withdrawalArr.length, 0);

        bytes32[] memory refundArr = map.incompleteRefunds();
        assertEq(refundArr.length, 0);

        // Verify reverse mappings
        (bytes32 queriedBurnAndBridge1, bytes32 queriedRefund, bool isSetBridgeTo) = map.getWithdrawalFromBridgeTo(l1);
        assertEq(queriedBurnAndBridge1, w1);
        assertEq(queriedRefund, l2);
        assertEq(isSetBridgeTo, true);

        (bytes32 queriedBurnAndBridge2, bytes32 queriedBridgeTo, bool isSetRefund) = map.getWithdrawalFromRefund(l2);
        assertEq(queriedBurnAndBridge2, w1);
        assertEq(queriedBridgeTo, l1);
        assertEq(isSetRefund, true);
    }

    function testDepositLinkUnsetHashReverts() public {
        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.UnsetHash.selector);
        map.linkDepositHashes(d1, l1);
    }

    function testWithdrawalLinkUnsetHashReverts() public {
        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.UnsetHash.selector);
        map.linkWithdrawalHashes(w1, l1);
    }

    function testRefundLinkUnsetHashReverts() public {
        vm.prank(relayer);
        vm.expectRevert(TxHashMapping.UnsetHash.selector);
        map.linkDepositHashes(w1, l2);
    }

    function testRefundLinkUnsetWithdrawalHashReverts() public {
        vm.startPrank(relayer);

        map.registerWithdrawal(w1);

        vm.expectRevert(TxHashMapping.UnsetHash.selector);
        map.linkDepositHashes(w1, l2);
        vm.stopPrank();
    }

    function testDepositLinkAlreadyLinkedReverts() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);
        map.linkDepositHashes(d1, l1);

        vm.expectRevert(TxHashMapping.AlreadyLinked.selector);
        map.linkDepositHashes(d1, l2);
        vm.stopPrank();
    }

    function testWithdrawalLinkAlreadyLinkedReverts() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.linkWithdrawalHashes(w1, l1);

        vm.expectRevert(TxHashMapping.AlreadyLinked.selector);
        map.linkWithdrawalHashes(w1, l2);
        vm.stopPrank();
    }

    function testRefundLinkAlreadyLinkedReverts() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.linkWithdrawalHashes(w1, l1);
        map.linkRefundHashes(w1, l2);

        vm.expectRevert(TxHashMapping.AlreadyLinked.selector);
        map.linkRefundHashes(w1, l3);
        vm.stopPrank();
    }

    // ---------- Swap-and-pop invariants ----------

    function testDepositSwapAndPopRemoveMiddle() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);
        map.registerDeposit(d2);
        map.registerDeposit(d3);

        // Remove middle (d2)
        map.linkDepositHashes(d2, l2);
        vm.stopPrank();

        bytes32[] memory arr = map.incompleteDeposits();
        assertEq(arr.length, 2);

        // Expect the remaining set is {d1,d3} (order may change)
        assertTrue(_contains(arr, d1));
        assertTrue(_contains(arr, d3));
        assertFalse(_contains(arr, d2));
    }

    function testWithdrawalSwapAndPopRemoveMiddle() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.registerWithdrawal(w2);
        map.registerWithdrawal(w3);

        // Remove middle (d2)
        map.linkWithdrawalHashes(w2, l2);
        vm.stopPrank();

        bytes32[] memory arr = map.incompleteWithdrawals();
        assertEq(arr.length, 2);

        // Expect the remaining set is {w1,w3} (order may change)
        assertTrue(_contains(arr, w1));
        assertTrue(_contains(arr, w3));
        assertFalse(_contains(arr, w2));
    }

    function testRefundSwapAndPopRemoveMiddle() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.registerWithdrawal(w2);
        map.registerWithdrawal(w3);

        // Remove middle (d2)
        map.linkWithdrawalHashes(w1, l1);
        map.linkWithdrawalHashes(w2, l2);
        map.linkWithdrawalHashes(w3, l3);

        map.linkRefundHashes(w2, l4);
        vm.stopPrank();

        bytes32[] memory arr = map.incompleteRefunds();
        assertEq(arr.length, 2);

        // Expect the remaining set is {w1,w3} (order may change)
        assertTrue(_contains(arr, w1));
        assertTrue(_contains(arr, w3));
        assertFalse(_contains(arr, w2));
    }

    function testDepositSwapAndPopSequence() public {
        vm.startPrank(relayer);
        map.registerDeposit(d1);
        map.registerDeposit(d2);
        map.registerDeposit(d3);

        // link first → array should now be [d3,d2] or [d2,d3]
        map.linkDepositHashes(d1, l1);

        // link last remaining element (one of them), array becomes length 1
        // choose the current arr[0] dynamically
        bytes32[] memory arr = map.incompleteDeposits();
        bytes32 toLink = arr[0] == d2 ? d2 : d3;
        bytes32 linked = arr[0] == d2 ? l2 : l3;
        map.linkDepositHashes(toLink, linked);
        vm.stopPrank();

        arr = map.incompleteDeposits();
        assertEq(arr.length, 1);
        // the only remaining should be the other one
        bytes32 remaining = (toLink == d2) ? d3 : d2;
        assertEq(arr[0], remaining);
    }

    function testWithdrawalSwapAndPopSequence() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.registerWithdrawal(w2);
        map.registerWithdrawal(w3);

        // link first → array should now be [w3,w2] or [w2,w3]
        map.linkWithdrawalHashes(w1, l1);

        // link last remaining element (one of them), array becomes length 1
        // choose the current arr[0] dynamically
        bytes32[] memory arr = map.incompleteWithdrawals();
        bytes32 toLink = arr[0] == w2 ? w2 : w3;
        bytes32 linked = arr[0] == w2 ? l2 : l3;
        map.linkWithdrawalHashes(toLink, linked);
        vm.stopPrank();

        arr = map.incompleteWithdrawals();
        assertEq(arr.length, 1);
        // the only remaining should be the other one
        bytes32 remaining = (toLink == w2) ? w3 : w2;
        assertEq(arr[0], remaining);
    }

    function testRefundSwapAndPopSequence() public {
        vm.startPrank(relayer);
        map.registerWithdrawal(w1);
        map.registerWithdrawal(w2);
        map.registerWithdrawal(w3);

        map.linkWithdrawalHashes(w1, l1);
        map.linkWithdrawalHashes(w2, l2);
        map.linkWithdrawalHashes(w3, l3);

        // link first → array should now be [w3,w2] or [w2,w3]
        map.linkRefundHashes(w1, l4);

        // link last remaining element (one of them), array becomes length 1
        // choose the current arr[0] dynamically
        bytes32[] memory arr = map.incompleteRefunds();
        bytes32 toLink = arr[0] == w2 ? w2 : w3;
        bytes32 linked = arr[0] == w2 ? l2 : l3;
        map.linkRefundHashes(toLink, linked);
        vm.stopPrank();

        arr = map.incompleteRefunds();
        assertEq(arr.length, 1);
        // the only remaining should be the other one
        bytes32 remaining = (toLink == w2) ? w3 : w2;
        assertEq(arr[0], remaining);
    }

    // ---------- Helpers ----------

    function _contains(bytes32[] memory arr, bytes32 x) internal pure returns (bool) {
        for (uint256 i; i < arr.length; i++) {
            if (arr[i] == x) return true;
        }
        return false;
    }
}
