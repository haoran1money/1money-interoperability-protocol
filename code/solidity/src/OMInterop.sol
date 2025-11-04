// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {IOMInterop} from "./IOMInterop.sol";

/**
 * @title OMInterop
 * @notice The interoperability contract described in ADR-001 with Ownable access control.
 */
contract OMInterop is Ownable, IOMInterop {
    struct TokenBinding {
        address omToken;
        address scToken;
        uint8 interopProtoId;
        bool exists;
    }

    struct CheckpointTally {
        uint32 certified;
        uint32 completed;
    }

    struct BridgeToRequest {
        address from;
        uint64 bbNonce;
        address to;
        uint256 amount;
        uint32 dstChainId;
        uint256 escrowFee;
        address omToken;
        uint64 checkpointId;
    }

    error Unauthorized();
    error InvalidAddress();
    error InvalidAmount();
    error UnknownToken(address token);
    error InvalidNonce(uint64 provided, uint64 expected);
    error InboundNonceOverflow();
    error CheckpointAlreadyCompleted(uint64 checkpointId);
    error InvalidChainId();
    error CheckpointCompleted(uint64 checkpointId);
    error NoCompletedCheckpoint();
    error InboundNonceUnavailable();

    /// @notice Temporary constant for the source chain identifier.
    uint32 internal constant SRC_CHAIN_ID = 1;

    /// @notice Operator address allowed to configure the contract.
    address public operator;
    /// @notice Relayer address allowed to execute bridge operations.
    address public relayer;
    /// @notice Next inbound nonce assigned to successful `bridgeFrom` executions.
    uint64 private _nextInboundNonce;
    /// @notice Earliest checkpoint index that is still incomplete (latest completed + 1).
    uint64 private _earliestIncompletedCheckpoint;
    mapping(address => TokenBinding) private _tokensBySidechain;
    mapping(address => TokenBinding) private _tokensByOm;
    mapping(address => uint64) private _latestBbNonce;
    mapping(uint64 => CheckpointTally) private _checkpointTallies;

    /// @notice Emitted when the operator address changes.
    event OperatorUpdated(address indexed newOperator);

    /// @notice Emitted when the relayer address changes.
    event RelayerUpdated(address indexed newRelayer);

    /// @notice Sets the initial owner, operator, and relayer.
    /// @param owner_ Address that will own the contract.
    /// @param operator_ Address allowed to execute operator-restricted actions.
    /// @param relayer_ Address allowed to dispatch bridge transactions.
    constructor(address owner_, address operator_, address relayer_)
        Ownable(owner_)
        nonZeroAddress(owner_)
        nonZeroAddress(operator_)
        nonZeroAddress(relayer_)
    {
        operator = operator_;
        relayer = relayer_;
        emit OperatorUpdated(operator_);
        emit RelayerUpdated(relayer_);
    }

    modifier onlyOperator() {
        _revertIfNotOperator();
        _;
    }

    modifier onlyRelayer() {
        _revertIfNotRelayer();
        _;
    }

    modifier nonZeroAddress(address account) {
        _revertIfZeroAddress(account);
        _;
    }

    modifier checkpointNotCompleted(uint64 checkpointId) {
        _ensureCheckpointNotCompleted(checkpointId);
        _;
    }

    modifier positiveAmount(uint256 amount) {
        _revertIfZeroAmount(amount);
        _;
    }

    modifier knownSidechainToken(address scToken) {
        _revertIfUnknownSidechainToken(scToken);
        _;
    }

    modifier recordInboundNonce() {
        _increaseInboundNonce();
        _;
    }

    modifier bridgeToValidated(BridgeToRequest memory req) {
        _bridgeToValidated(req);
        _;
    }

    /// @notice Updates the operator account. Only callable by the owner.
    /// @param newOperator Address of the new operator.
    function setOperator(address newOperator) external onlyOwner nonZeroAddress(newOperator) {
        operator = newOperator;
        emit OperatorUpdated(newOperator);
    }

    /// @notice Updates the relayer account. Only callable by the owner.
    /// @param newRelayer Address of the new relayer.
    function setRelayer(address newRelayer) external onlyOwner nonZeroAddress(newRelayer) {
        relayer = newRelayer;
        emit RelayerUpdated(newRelayer);
    }

    /// @inheritdoc IOMInterop
    function bridgeFrom(address to, uint256 amount)
        external
        override
        nonZeroAddress(to)
        positiveAmount(amount)
        knownSidechainToken(msg.sender)
        recordInboundNonce
    {
        TokenBinding storage binding = _tokensBySidechain[msg.sender];
        emit OMInteropReceived(_latestInboundNonceInternal(), to, amount, binding.omToken, SRC_CHAIN_ID);
    }

    /// @inheritdoc IOMInterop
    function bridgeTo(
        address from,
        uint64 bbNonce,
        address to,
        uint256 amount,
        uint32 dstChainId,
        uint256 escrowFee,
        address omToken,
        uint64 checkpointId
    ) external override onlyRelayer checkpointNotCompleted(checkpointId) {
        BridgeToRequest memory req = BridgeToRequest({
            from: from,
            bbNonce: bbNonce,
            to: to,
            amount: amount,
            dstChainId: dstChainId,
            escrowFee: escrowFee,
            omToken: omToken,
            checkpointId: checkpointId
        });

        _bridgeTo(req);
    }

    /// @inheritdoc IOMInterop
    function updateCheckpointInfo(uint64 checkpointId, uint32 burnAndBridgeCount)
        external
        override
        onlyRelayer
        checkpointNotCompleted(checkpointId)
    {
        CheckpointTally storage tally = _checkpointTallies[checkpointId];

        tally.certified = burnAndBridgeCount;

        _tryCompleteCheckpoint(checkpointId);
    }

    /// @inheritdoc IOMInterop
    function getLatestCompletedCheckpoint() external view override returns (uint64 checkpointId) {
        checkpointId = _earliestIncompletedCheckpoint;
        if (checkpointId == 0) revert NoCompletedCheckpoint();

        unchecked {
            checkpointId -= 1;
        }
    }

    /// @inheritdoc IOMInterop
    function mapTokenAddresses(address omToken, address scToken, uint8 interopProtoId)
        external
        override
        onlyOperator
        nonZeroAddress(omToken)
        nonZeroAddress(scToken)
    {
        TokenBinding memory binding = TokenBinding({
            omToken: omToken, scToken: scToken, interopProtoId: interopProtoId, exists: true
        });

        _tokensBySidechain[scToken] = binding;
        _tokensByOm[omToken] = binding;
    }

    /// @inheritdoc IOMInterop
    function getTokenBindingForOm(address omToken)
        external
        view
        override
        returns (address scToken, uint8 interopProtoId, bool exists)
    {
        TokenBinding storage binding = _tokensByOm[omToken];
        return (binding.scToken, binding.interopProtoId, binding.exists);
    }

    /// @inheritdoc IOMInterop
    function getTokenBindingForSidechain(address scToken)
        external
        view
        override
        returns (address omToken, uint8 interopProtoId, bool exists)
    {
        TokenBinding storage binding = _tokensBySidechain[scToken];
        return (binding.omToken, binding.interopProtoId, binding.exists);
    }

    /// @inheritdoc IOMInterop
    function getLatestInboundNonce() external view override returns (uint64 nonce) {
        nonce = _latestInboundNonceInternal();
    }

    /// @inheritdoc IOMInterop
    function getLatestProcessedNonce(address account) external view override returns (uint64 nonce) {
        nonce = _latestBbNonce[account];
    }

    /// @inheritdoc IOMInterop
    function getCheckpointTally(uint64 checkpointId)
        external
        view
        override
        returns (uint32 certified, uint32 completed)
    {
        CheckpointTally storage tally = _checkpointTallies[checkpointId];
        certified = tally.certified;
        completed = tally.completed;
    }

    function _revertIfNotOperator() internal view {
        if (operator != msg.sender) revert Unauthorized();
    }

    function _revertIfNotRelayer() internal view {
        if (relayer != msg.sender) revert Unauthorized();
    }

    function _revertIfZeroAddress(address account) internal pure {
        if (account == address(0)) revert InvalidAddress();
    }

    function _revertIfZeroAmount(uint256 amount) internal pure {
        if (amount == 0) revert InvalidAmount();
    }

    function _revertIfUnknownSidechainToken(address scToken) internal view {
        if (!_tokensBySidechain[scToken].exists) revert UnknownToken(scToken);
    }

    function _revertIfUnknownOmToken(address omToken) internal view {
        if (!_tokensByOm[omToken].exists) revert UnknownToken(omToken);
    }

    function _enforceSequentialNonce(address account, uint64 bbNonce) internal {
        uint64 expected = _latestBbNonce[account] + 1;
        if (bbNonce != expected) revert InvalidNonce(bbNonce, expected);
        _latestBbNonce[account] = bbNonce;
    }

    function _revertIfInvalidChain(uint32 chainId) internal pure {
        if (chainId == 0) revert InvalidChainId();
    }

    function _increaseInboundNonce() internal {
        if (_nextInboundNonce == type(uint64).max) revert InboundNonceOverflow();
        unchecked {
            _nextInboundNonce += 1;
        }
    }

    function _revertIfCheckpointComplete(uint64 checkpointId) internal view {
        CheckpointTally storage tally = _checkpointTallies[checkpointId];
        if (tally.certified != 0 && tally.completed >= tally.certified) {
            revert CheckpointAlreadyCompleted(checkpointId);
        }
    }

    function _bridgeToValidated(BridgeToRequest memory req) internal {
        _ensureCheckpointNotCompleted(req.checkpointId);
        _revertIfZeroAddress(req.from);
        _revertIfZeroAddress(req.to);
        _revertIfZeroAddress(req.omToken);
        _revertIfZeroAmount(req.amount);
        _revertIfInvalidChain(req.dstChainId);
        _revertIfUnknownOmToken(req.omToken);
        _enforceSequentialNonce(req.from, req.bbNonce);
        _revertIfCheckpointComplete(req.checkpointId);
    }

    function _bridgeTo(BridgeToRequest memory req) internal bridgeToValidated(req) recordInboundNonce {
        _recordCheckpointProgress(req.checkpointId);
        // TODO: invoke cross-chain bridge contract with req parameters before emitting event.
        // For now, we refund the full escrowFee.
        emit OMInteropSent(_latestInboundNonceInternal(), req.from, req.escrowFee, req.omToken, req.dstChainId);
    }

    function _recordCheckpointProgress(uint64 checkpointId) internal {
        CheckpointTally storage tally = _checkpointTallies[checkpointId];
        tally.completed += 1;
        _tryCompleteCheckpoint(checkpointId);
    }

    function _tryCompleteCheckpoint(uint64 checkpointId) internal {
        CheckpointTally storage tally = _checkpointTallies[checkpointId];
        if (tally.certified == 0 || tally.completed < tally.certified) {
            return;
        }

        uint64 earliest = _earliestIncompletedCheckpoint;
        if (checkpointId + 1 > earliest) {
            if (earliest != 0) {
                unchecked {
                    delete _checkpointTallies[earliest - 1];
                }
            }

            _earliestIncompletedCheckpoint = checkpointId + 1;
        }
    }

    function _ensureCheckpointNotCompleted(uint64 checkpointId) internal view {
        if (checkpointId < _earliestIncompletedCheckpoint) {
            revert CheckpointCompleted(checkpointId);
        }
    }

    function _latestInboundNonceInternal() internal view returns (uint64) {
        uint64 next = _nextInboundNonce;
        if (next == 0) {
            revert InboundNonceUnavailable();
        }
        unchecked {
            return next - 1;
        }
    }
}
