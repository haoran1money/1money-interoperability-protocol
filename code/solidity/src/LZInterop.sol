// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {IOFT, SendParam} from "@layerzerolabs/oft-evm/contracts/interfaces/IOFT.sol";
import {OptionsBuilder} from "@layerzerolabs/oapp-evm/contracts/oapp/libs/OptionsBuilder.sol";
import {
    ILayerZeroEndpointV2,
    MessagingFee
} from "@layerzerolabs/lz-evm-protocol-v2/contracts/interfaces/ILayerZeroEndpointV2.sol";
import {IOAppCore} from "@layerzerolabs/oapp-evm/contracts/oapp/interfaces/IOAppCore.sol";
import {BridgeToRequest} from "./IOMInterop.sol";

/**
 * @title LZInterop
 * @notice LayerZero-specific functionality shared by OMInterop.
 */
abstract contract LZInterop {
    using SafeERC20 for IERC20;

    error InvalidBridgeData();

    function _quoteLayerZero(address oftToken, BridgeToRequest memory req)
        internal
        view
        returns (uint256 bridgeFee, address feeToken)
    {
        SendParam memory sendParam = _buildLayerZeroSendParam(req);
        MessagingFee memory fee = IOFT(oftToken).quoteSend(sendParam, true);
        feeToken = IOAppCore(oftToken).endpoint().lzToken();
        bridgeFee = fee.lzTokenFee;
    }

    function _bridgeWithLayerZero(address oftToken, BridgeToRequest memory req) internal {
        SendParam memory sendParam = _buildLayerZeroSendParam(req);
        MessagingFee memory fee = IOFT(oftToken).quoteSend(sendParam, true);
        address relayer = msg.sender;
        _collectLzFeeFromRelayer(oftToken, fee.lzTokenFee, relayer);
        IOFT(oftToken).send(sendParam, fee, relayer);
    }

    function _collectLzFeeFromRelayer(address oftToken, uint256 lzTokenFee, address relayer) internal {
        if (lzTokenFee == 0) return;
        ILayerZeroEndpointV2 endpoint = IOAppCore(oftToken).endpoint();
        address lzToken = endpoint.lzToken();
        IERC20 token = IERC20(lzToken);
        token.safeTransferFrom(relayer, address(this), lzTokenFee);
        token.forceApprove(oftToken, lzTokenFee);
    }

    function _buildLayerZeroSendParam(BridgeToRequest memory req) internal view returns (SendParam memory sendParam) {
        (uint128 maxGas, uint256 minAmountLd) = _decodeBridgeData(req);
        uint128 gasLimit = maxGas;
        bytes memory options = OptionsBuilder.newOptions();
        options = OptionsBuilder.addExecutorLzReceiveOption(options, gasLimit, 0);

        sendParam = SendParam({
            dstEid: req.dstChainId,
            to: _addressToBytes32(req.to),
            amountLD: req.amount,
            minAmountLD: minAmountLd,
            extraOptions: options,
            composeMsg: bytes(""),
            oftCmd: bytes("")
        });
    }

    function _addressToBytes32(address account) internal pure returns (bytes32) {
        return bytes32(uint256(uint160(account)));
    }

    function _decodeBridgeData(BridgeToRequest memory req)
        internal
        view
        returns (uint128 maxGas, uint256 minAmountLd)
    {
        try this._decodeBridgeDataStrict(req.bridgeData) returns (uint128 gas, uint256 minAmount) {
            maxGas = gas;
            minAmountLd = minAmount;
        } catch {
            revert InvalidBridgeData();
        }
    }

    function _decodeBridgeDataStrict(bytes calldata bridgeData)
        external
        pure
        returns (uint128 maxGas, uint256 minAmountLd)
    {
        (maxGas, minAmountLd) = abi.decode(bridgeData, (uint128, uint256));
    }
}
