// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

interface IOMInterop {
    /// @notice Emitted when an incoming bridge transfer is registered on the sidechain.
    event OMInteropReceived(uint64 nonce, address indexed to, uint256 amount, address indexed omToken);

    /// @notice Emitted when an outgoing bridge transfer is dispatched to an external chain.
    event OMInteropSent(uint64 nonce, address indexed from, uint256 refundAmount, address indexed omToken);

    /// @notice Records an incoming cross-chain transfer emitted by the token bridge.
    /// @param to 1Money account receiving the bridged funds.
    /// @param amount Amount of tokens to be minted on the payment lane.
    function bridgeFrom(address to, uint256 amount) external;

    /// @notice Dispatches an outgoing message through the configured cross-chain protocol.
    /// @param from 1Money account burning funds on the payment lane.
    /// @param bbNonce Sequential BurnAndBridge nonce supplied by the relayer.
    /// @param to Recipient on the external chain.
    /// @param amount Amount to bridge out.
    /// @param dstChainId Destination chain identifier understood by the bridge.
    /// @param escrowFee Portion of escrowed fee to refund to `from`.
    /// @param omToken Token address on the payment network.
    /// @param checkpointId Checkpoint that certified the originating BurnAndBridge.
    function bridgeTo(
        address from,
        uint64 bbNonce,
        address to,
        uint256 amount,
        uint32 dstChainId,
        uint256 escrowFee,
        address omToken,
        uint64 checkpointId
    ) external;

    /// @notice Updates the contract with the number of certified BurnAndBridge instructions in a checkpoint.
    /// @param checkpointId Identifier of the payment-network checkpoint.
    /// @param burnAndBridgeCount Count of certified BurnAndBridge transactions included in the checkpoint.
    function updateCheckpointInfo(uint64 checkpointId, uint32 burnAndBridgeCount) external;

    /// @notice Returns the latest checkpoint for which every BurnAndBridge has been completed.
    /// @dev Reverts if no checkpoint has been completed yet.
    function getLatestCompletedCheckpoint() external view returns (uint64 checkpointId);

    /// @notice Maps a payment-network token to its sidechain counterpart for a given interop protocol.
    /// @param omToken Token address on the payment network.
    /// @param scToken Token address deployed on the sidechain.
    /// @param interopProtoId Identifier for the cross-chain protocol.
    function mapTokenAddresses(address omToken, address scToken, uint8 interopProtoId) external;

    /// @notice Retrieves the sidechain mapping details for a payment-network token.
    /// @return scToken Sidechain token address.
    /// @return interopProtoId Identifier of the interop protocol.
    /// @return exists True when a mapping was registered.
    function getTokenBindingForOm(address omToken)
        external
        view
        returns (address scToken, uint8 interopProtoId, bool exists);

    /// @notice Retrieves the payment-network mapping details for a sidechain token.
    /// @return omToken Payment-network token address.
    /// @return interopProtoId Identifier of the interop protocol.
    /// @return exists True when a mapping was registered.
    function getTokenBindingForSidechain(address scToken)
        external
        view
        returns (address omToken, uint8 interopProtoId, bool exists);

    /// @notice Latest inbound nonce produced by successful `bridgeFrom` calls.
    function getLatestInboundNonce() external view returns (uint64 nonce);

    /// @notice Latest processed BurnAndBridge nonce per account.
    function getLatestProcessedNonce(address account) external view returns (uint64 nonce);

    /// @notice Returns the certified and completed counters for a checkpoint.
    function getCheckpointTally(uint64 checkpointId) external view returns (uint32 certified, uint32 completed);
}
