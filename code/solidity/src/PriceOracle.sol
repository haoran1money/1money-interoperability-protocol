// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ERC165} from "@openzeppelin/contracts/utils/introspection/ERC165.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {IPriceOracle} from "./IPriceOracle.sol";

/**
 * @title PriceOracle
 * @notice Stores and provides the value of 1e18 native gas token expressed in supported tokens.
 */
contract PriceOracle is Ownable, ERC165, IPriceOracle {
    // ---------- Constants ----------
    uint256 constant PRICE_DECIMALS = 1e18;

    // ---------- Errors ----------
    error Unauthorized();
    error InvalidAddress();
    error UnsetToken(address token);
    error LengthMismatch(uint256 left, uint256 right);

    // ---------- Events ----------
    event OperatorUpdated(address indexed oldOperator, address indexed newOperator);

    event PriceUpdated(address indexed token, uint256 oldValue, uint256 newValue);

    // ---------- Access ----------
    address public operator;

    modifier onlyOperatorOrOwner() {
        _onlyOperatorOrOwner();
        _;
    }

    function _onlyOperatorOrOwner() internal view {
        if (msg.sender != operator && msg.sender != owner()) revert Unauthorized();
    }

    modifier nonZeroAddress(address account) {
        _nonZeroAddress(account);
        _;
    }

    function _nonZeroAddress(address account) internal pure {
        if (account == address(0)) revert InvalidAddress();
    }

    // ---------- ERC-165 ----------
    function supportsInterface(bytes4 interfaceId)
        public
        view
        virtual
        override(ERC165, IERC165) // now ERC165 *is* a parent
        returns (bool)
    {
        return interfaceId == type(IPriceOracle).interfaceId || super.supportsInterface(interfaceId);
    }

    // ---------- Storage ----------
    mapping(address => uint256) private _priceMapping;
    mapping(address => uint256) private _lastUpdateMapping;

    // ---------- Constructor ----------
    constructor(address owner_, address operator_) Ownable(owner_) nonZeroAddress(owner_) nonZeroAddress(operator_) {
        operator = operator_;
        emit OperatorUpdated(address(0), operator_);
    }

    // ---------- Admin ----------
    function setOperator(address newOperator) external onlyOwner nonZeroAddress(newOperator) {
        emit OperatorUpdated(operator, newOperator);
        operator = newOperator;
    }

    // ---------- Core API ----------
    /**
     * @notice Updates the conversion rate between the native gas token and a given token,
     *         expressed in 1e18-scaled units.
     *
     * @dev The `value` parameter represents how many units of `token` correspond to
     *      1e18 units of the native gas token.
     *
     *      Examples:
     *        - If 1 native token = 2 TOKEN, then:
     *              value = 2e18
     *
     *      The price is stored, using 1e18 fixed-point
     *      arithmetic (no floating point is used in Solidity).
     *
     * @param token The token whose price mapping is being updated.
     * @param value The 1e18-scaled amount of `token` obtained per 1e18 native tokens.
     */
    function updatePrice(address token, uint256 value) external nonZeroAddress(token) onlyOperatorOrOwner {
        _updatePrice(token, value);
    }

    /// @notice Updates the conversion rates between the native gas token and a list of tokens and prices
    /// @param tokens The token list whose price mapping are being updated.
    /// @param values The 1e18-scaled amount of `token` obtained per 1e18 native tokens for each token in the list `tokens`.
    function bulkUpdatePrices(address[] calldata tokens, uint256[] calldata values) external onlyOperatorOrOwner {
        uint256 len = tokens.length;
        if (len != values.length) revert LengthMismatch(len, values.length);
        for (uint256 i = 0; i < len; ++i) {
            address token = tokens[i];
            if (token == address(0)) revert InvalidAddress();
            _updatePrice(token, values[i]);
        }
    }

    // ---------- Views ----------

    /// @notice Convert an arbitrary amount of native token in units of `token`.
    /// @param token ERC20 token address.
    /// @param nativeAmount Amount of native token (in wei).
    /// @return tokenAmount Equivalent amount of `token` (in wei), using the stored price.
    function convertNativeTokenToToken(address token, uint256 nativeAmount)
        external
        view
        returns (uint256 tokenAmount, uint256 lastUpdate)
    {
        lastUpdate = _lastUpdateMapping[token];
        if (lastUpdate == 0) revert UnsetToken(token);
        uint256 price = _priceMapping[token];

        // nativeAmount * price / 1e18
        tokenAmount = (nativeAmount * price) / PRICE_DECIMALS;
    }

    // ---------- Internals ----------
    /**
     * @notice Updates the conversion rate between the native gas token and a given token,
     *         expressed in 1e18-scaled units.
     *
     * @dev The `value` parameter represents how many units of `token` correspond to
     *      1e18 units of the native gas token.
     *
     *      Examples:
     *        - If 1 native token = 2 TOKEN, then:
     *              value = 2e18
     *
     *      The price is stored, using 1e18 fixed-point
     *      arithmetic (no floating point is used in Solidity).
     *
     * @param token The token whose price mapping is being updated.
     * @param value The 1e18-scaled amount of `token` obtained per 1e18 native tokens.
     */
    function _updatePrice(address token, uint256 value) internal {
        emit PriceUpdated(token, _priceMapping[token], value);

        _priceMapping[token] = value;
        _lastUpdateMapping[token] = block.timestamp;
    }
}
