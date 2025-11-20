// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Test} from "forge-std/Test.sol";
import {PriceOracle} from "../src/PriceOracle.sol";

contract PriceOracleTest is Test {
    PriceOracle oracle;

    address constant OWNER = address(0xA11CE);
    address constant OPERATOR = address(0xB0B);
    address constant TOKEN_A = address(0x1234);
    address constant TOKEN_B = address(0xBEEF);

    function setUp() public {
        oracle = new PriceOracle(OWNER, OPERATOR);
    }

    // ---------- Test constructor ----------
    function testConstructorSetsOwnerAndOperator() public view {
        assertEq(oracle.owner(), OWNER);
        assertEq(oracle.operator(), OPERATOR);
    }

    function testConstructorEmitsOperatorUpdated() public {
        vm.expectEmit(true, true, false, false);
        emit PriceOracle.OperatorUpdated(address(0), OPERATOR);
        new PriceOracle(OWNER, OPERATOR);
    }

    function testConstructorRevertsOnZeroOwner() public {
        vm.expectRevert(abi.encodeWithSignature("OwnableInvalidOwner(address)", address(0)));
        new PriceOracle(address(0), OPERATOR);
    }

    function testConstructorRevertsOnZeroOperator() public {
        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        new PriceOracle(OWNER, address(0));
    }

    // ---------- Test setting operator ----------
    function testSetOperatorByOwner() public {
        vm.prank(OWNER);
        vm.expectEmit(true, true, false, false);
        emit PriceOracle.OperatorUpdated(OPERATOR, address(0xC0DE));

        oracle.setOperator(address(0xC0DE));
        assertEq(oracle.operator(), address(0xC0DE));
    }

    function testSetOperatorRevertsIfNotOwner() public {
        vm.prank(OPERATOR);
        vm.expectRevert(abi.encodeWithSignature("OwnableUnauthorizedAccount(address)", OPERATOR));
        oracle.setOperator(address(0xC0DE));
    }

    function testSetOperatorRevertsOnZero() public {
        vm.prank(OWNER);
        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        oracle.setOperator(address(0));
    }

    // ---------- Test price updating ----------
    function testUpdatePriceByOperator() public {
        uint256 oldPrice = 0;
        uint256 newPrice = 1e18;

        vm.prank(OPERATOR);
        vm.expectEmit(true, true, true, true);
        emit PriceOracle.PriceUpdated(TOKEN_A, oldPrice, newPrice);

        oracle.updatePrice(TOKEN_A, newPrice);

        (uint256 price,) = oracle.convertNativeTokenToToken(TOKEN_A, 3);

        assertEq(price, 3);
    }

    function testUpdatePriceByOwner() public {
        uint256 oldPrice = 0;
        uint256 newPrice = 1e18;

        vm.prank(OWNER);
        vm.expectEmit(true, true, true, true);
        emit PriceOracle.PriceUpdated(TOKEN_A, oldPrice, newPrice);

        oracle.updatePrice(TOKEN_A, newPrice);

        (uint256 price,) = oracle.convertNativeTokenToToken(TOKEN_A, 3);

        assertEq(price, 3);
    }

    function testUpdatePriceTwiceUpdatesOldValueInEvent() public {
        uint256 first = 1e18;
        uint256 second = 2e18;

        vm.startPrank(OPERATOR);
        oracle.updatePrice(TOKEN_A, first);

        vm.expectEmit(true, true, true, true);
        emit PriceOracle.PriceUpdated(TOKEN_A, first, second);
        oracle.updatePrice(TOKEN_A, second);
        vm.stopPrank();

        (uint256 price,) = oracle.convertNativeTokenToToken(TOKEN_A, 3);

        assertEq(price, 6);
    }

    function testUpdatePriceRevertsOnZeroToken() public {
        vm.prank(OPERATOR);
        vm.expectRevert(abi.encodeWithSignature("InvalidAddress()"));
        oracle.updatePrice(address(0), 1e18);
    }

    function testUpdateBulkPrices() public {
        address[] memory tokens = new address[](2);
        tokens[0] = TOKEN_A;
        tokens[1] = TOKEN_B;

        uint256[] memory values = new uint256[](2);
        values[0] = 1e18;
        values[1] = 2e18;

        vm.prank(OPERATOR);

        vm.expectEmit(true, false, false, true);
        emit PriceOracle.PriceUpdated(tokens[0], 0, values[0]);

        vm.expectEmit(true, false, false, true);
        emit PriceOracle.PriceUpdated(tokens[1], 0, values[1]);

        oracle.bulkUpdatePrices(tokens, values);

        (uint256 priceA,) = oracle.convertNativeTokenToToken(TOKEN_A, 3);
        (uint256 priceB,) = oracle.convertNativeTokenToToken(TOKEN_B, 3);

        assertEq(priceA, 3);
        assertEq(priceB, 6);
    }

    // ---------- Test get token price ----------
    function testGetTokenPriceUnsetReverts() public {
        vm.expectRevert(abi.encodeWithSignature("UnsetToken(address)", TOKEN_A));
        oracle.convertNativeTokenToToken(TOKEN_A, 3);
    }

    function testGetTokenPriceAfterUpdate() public {
        vm.prank(OPERATOR);
        oracle.updatePrice(TOKEN_A, 42e18);

        (uint256 price,) = oracle.convertNativeTokenToToken(TOKEN_A, 3);

        assertEq(price, 126);
    }
}
