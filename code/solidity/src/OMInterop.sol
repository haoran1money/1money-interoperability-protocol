// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {IOMInterop, InteropProtocol, BridgeToRequest} from "./IOMInterop.sol";
import {IPriceOracle} from "./IPriceOracle.sol";
import {LZInterop} from "./LZInterop.sol";
import {OMInteropTypes} from "./OMInteropTypes.sol";
import {OMInteropStorage} from "./OMInteropStorage.sol";

/**
 * @title OMInterop
 * @notice The interoperability contract described in ADR-001 with Ownable access control.
 */
contract OMInterop is OwnableUpgradeable, LZInterop, UUPSUpgradeable, IOMInterop {
    IPriceOracle public priceOracle;

    // Allow the contract to receive native tokens for LayerZero fees
    receive() external payable {}

    error Unauthorized();
    error InvalidAddress();
    error InvalidAmount();
    error UnknownToken(address token);
    error InvalidNonce(uint64 provided, uint64 expected);
    error InboundNonceOverflow();
    error CheckpointAlreadyCompleted(uint64 checkpointId);
    error CheckpointCompletedAndPruned(uint64 checkpointId);
    error InvalidChainId();
    error CheckpointCompleted(uint64 checkpointId);
    error CheckpointAlreadyRegistered(uint64 checkpointId);
    error InboundNonceUnavailable();
    error UnknownInteropProto(InteropProtocol interopProtoId);
    error RateLimitExceeded();
    error EscrowFeeTooLow(uint256 provided, uint256 required);

    /// @notice Temporary constant for the source chain identifier.
    uint32 internal constant SRC_CHAIN_ID = 1;

    /// @notice Emitted when the operator address changes.
    event OperatorUpdated(address indexed newOperator);

    /// @notice Emitted when the relayer address changes.
    event RelayerUpdated(address indexed newRelayer);

    /// @notice Emitted when the Price Oracle address changes.
    event PriceOracleUpdated(address indexed newPriceOracle);

    /// @notice Emitted when the rate limit changed for a token.
    event RateLimitsChanged(address token, uint256 limit, uint256 window);

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @notice Sets the initial owner, operator, and relayer.
    /// @param owner_ Address that will own the contract.
    /// @param operator_ Address allowed to execute operator-restricted actions.
    /// @param relayer_ Address allowed to dispatch bridge transactions.
    function initialize(address owner_, address operator_, address relayer_, address priceOracle_)
        external
        initializer
        nonZeroAddress(owner_)
        nonZeroAddress(operator_)
        nonZeroAddress(relayer_)
        nonZeroAddress(priceOracle_)
    {
        __Ownable_init(owner_);

        OMInteropStorage.Layout storage s = OMInteropStorage.layout();

        s.operator = operator_;
        s.relayer = relayer_;
        s.priceOracle = priceOracle_;

        // IMPORTANT: Checkpoint 0 will not have any user transactions,
        // so we initialize earliestIncompletedCheckpoint to 1
        s.earliestIncompletedCheckpoint = 1;

        emit OperatorUpdated(operator_);
        emit RelayerUpdated(relayer_);
        emit PriceOracleUpdated(priceOracle_);
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    function operator() external view returns (address) {
        return OMInteropStorage.layout().operator;
    }

    function relayer() external view returns (address) {
        return OMInteropStorage.layout().relayer;
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

    function version() external pure virtual returns (string memory) {
        return "v1.0.0";
    }

    /// @notice Updates the operator account. Only callable by the owner.
    /// @param newOperator Address of the new operator.
    function setOperator(address newOperator) external onlyOwner nonZeroAddress(newOperator) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        s.operator = newOperator;
        emit OperatorUpdated(newOperator);
    }

    /// @notice Updates the relayer account. Only callable by the owner.
    /// @param newRelayer Address of the new relayer.
    function setRelayer(address newRelayer) external onlyOwner nonZeroAddress(newRelayer) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        s.relayer = newRelayer;
        emit RelayerUpdated(newRelayer);
    }

    /// @notice Updates the Price Oracle address. Only callable by the owner.
    /// @param newPriceOracle Address of the new Price Oracle.
    function setPriceOracle(address newPriceOracle) external onlyOwner nonZeroAddress(newPriceOracle) {
        // Basic contract check
        require(newPriceOracle.code.length > 0, "Not a contract");

        // ERC-165 interface check
        require(
            IERC165(newPriceOracle).supportsInterface(type(IPriceOracle).interfaceId), "Does not implement IPriceOracle"
        );

        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        s.priceOracle = newPriceOracle;
        emit PriceOracleUpdated(newPriceOracle);
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
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding storage binding = s.tokensBySidechain[msg.sender];
        // Enforce inflow limit
        _checkAndUpdateRateLimit(binding.omToken, amount);

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
        uint64 checkpointId,
        bytes calldata bridgeData,
        bytes32 sourceHash
    ) external override onlyRelayer checkpointNotCompleted(checkpointId) {
        BridgeToRequest memory req = BridgeToRequest({
            from: from,
            bbNonce: bbNonce,
            to: to,
            amount: amount,
            dstChainId: dstChainId,
            escrowFee: escrowFee,
            omToken: omToken,
            checkpointId: checkpointId,
            bridgeData: bridgeData,
            sourceHash: sourceHash
        });

        _bridgeTo(req);
    }

    /// @inheritdoc IOMInterop
    function quoteBridgeTo(
        address from,
        uint64 bbNonce,
        address to,
        uint256 amount,
        uint32 dstChainId,
        uint256 escrowFee,
        address omToken,
        uint64 checkpointId,
        bytes calldata bridgeData,
        bytes32 sourceHash
    ) public view override returns (uint256 bridgeFee) {
        BridgeToRequest memory req = BridgeToRequest({
            from: from,
            bbNonce: bbNonce,
            to: to,
            amount: amount,
            dstChainId: dstChainId,
            escrowFee: escrowFee,
            omToken: omToken,
            checkpointId: checkpointId,
            bridgeData: bridgeData,
            sourceHash: sourceHash
        });

        _quoteBridgeToValidated(req);
        bridgeFee = _quoteBridgeTo(req);
    }

    /// @inheritdoc IOMInterop
    function updateCheckpointInfo(uint64 checkpointId, bytes32[] calldata burnAndBridgeHashes)
        external
        override
        onlyRelayer
        checkpointNotCompleted(checkpointId)
    {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.CheckpointTally storage tally = s.checkpointTallies[checkpointId];

        // Ensure checkpoint is not already fully registered
        if (tally.certified != 0 && tally.completed >= tally.certified) {
            revert CheckpointAlreadyRegistered(checkpointId);
        }

        // Iterate through all provided burnAndBridge hashes
        for (uint256 i = 0; i < burnAndBridgeHashes.length; i++) {
            bytes32 burnAndBridgeHash = burnAndBridgeHashes[i];

            // Ensure transaction hash is only recorded once
            if (s.checkpointIdsBySourceHash[burnAndBridgeHash] == 0) {
                s.checkpointIdsBySourceHash[burnAndBridgeHash] = checkpointId;
                tally.certified += 1;

                // If the transaction was already processed, count it as completed
                if (s.processedBridges[burnAndBridgeHash]) {
                    tally.completed += 1;
                }
            }
        }

        _tryPruneCheckpoint(checkpointId);
    }

    /// @inheritdoc IOMInterop
    function getLatestCompletedCheckpoint() external view override returns (uint64 checkpointId) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        checkpointId = s.earliestIncompletedCheckpoint;

        // Avoid underflow
        if (checkpointId == 0) {
            return 0;
        }

        unchecked {
            checkpointId -= 1;
        }
    }

    /// @inheritdoc IOMInterop
    function mapTokenAddresses(address omToken, address scToken, InteropProtocol interopProtoId)
        external
        override
        onlyOperator
        nonZeroAddress(omToken)
        nonZeroAddress(scToken)
    {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding memory binding = OMInteropTypes.TokenBinding({
            omToken: omToken, scToken: scToken, interopProtoId: interopProtoId, exists: true
        });

        s.tokensBySidechain[scToken] = binding;
        s.tokensByOm[omToken] = binding;
    }

    /// @inheritdoc IOMInterop
    function getTokenBindingForOm(address omToken)
        external
        view
        override
        returns (address scToken, InteropProtocol interopProtoId, bool exists)
    {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding storage binding = s.tokensByOm[omToken];
        return (binding.scToken, binding.interopProtoId, binding.exists);
    }

    /// @inheritdoc IOMInterop
    function getTokenBindingForSidechain(address scToken)
        external
        view
        override
        returns (address omToken, InteropProtocol interopProtoId, bool exists)
    {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding storage binding = s.tokensBySidechain[scToken];
        return (binding.omToken, binding.interopProtoId, binding.exists);
    }

    /// @inheritdoc IOMInterop
    function getLatestInboundNonce() external view override returns (uint64 nonce) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        nonce = s.nextInboundNonce;
    }

    /// @inheritdoc IOMInterop
    function getLatestProcessedNonce(address account) external view override returns (uint64 nonce) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        nonce = s.latestBbNonce[account];
    }

    /// @inheritdoc IOMInterop
    function getCheckpointTally(uint64 checkpointId)
        external
        view
        override
        returns (uint32 certified, uint32 completed)
    {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.CheckpointTally storage tally = s.checkpointTallies[checkpointId];
        certified = tally.certified;
        completed = tally.completed;
    }

    function _revertIfNotOperator() internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (s.operator != msg.sender) revert Unauthorized();
    }

    function _revertIfNotRelayer() internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (s.relayer != msg.sender) revert Unauthorized();
    }

    function _revertIfZeroAddress(address account) internal pure {
        if (account == address(0)) revert InvalidAddress();
    }

    function _revertIfZeroAmount(uint256 amount) internal pure {
        if (amount == 0) revert InvalidAmount();
    }

    function _revertIfUnknownSidechainToken(address scToken) internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (!s.tokensBySidechain[scToken].exists) revert UnknownToken(scToken);
    }

    function _revertIfUnknownOmToken(address omToken) internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (!s.tokensByOm[omToken].exists) revert UnknownToken(omToken);
    }

    function _enforceSequentialNonce(address account, uint64 bbNonce) internal {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        uint64 expected = s.latestBbNonce[account];
        if (bbNonce != expected) revert InvalidNonce(bbNonce, expected);
        s.latestBbNonce[account] = bbNonce + 1;
    }

    function _revertIfInvalidChain(uint32 chainId) internal pure {
        if (chainId == 0) revert InvalidChainId();
    }

    function _increaseInboundNonce() internal {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (s.nextInboundNonce == type(uint64).max) revert InboundNonceOverflow();
        unchecked {
            s.nextInboundNonce += 1;
        }
    }

    function _revertIfCheckpointComplete(uint64 checkpointId) internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.CheckpointTally storage tally = s.checkpointTallies[checkpointId];
        if (tally.certified != 0 && tally.completed >= tally.certified) {
            revert CheckpointCompleted(checkpointId);
        }
    }

    function _quoteBridgeTo(BridgeToRequest memory req) internal view returns (uint256 bridgeFee) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding storage binding = s.tokensByOm[req.omToken];
        bridgeFee = _quoteBridgeTo(binding, req);
    }

    function _quoteBridgeTo(OMInteropTypes.TokenBinding storage binding, BridgeToRequest memory req)
        internal
        view
        returns (uint256 bridgeFee)
    {
        InteropProtocol protoId = binding.interopProtoId;
        if (protoId == InteropProtocol.LayerZero) {
            uint256 rawFee;
            rawFee = _quoteLayerZero(binding.scToken, req);
            // Compute L1 token fee based on price oracle
            bridgeFee = _computeFee(binding.omToken, rawFee);
        } else if (protoId == InteropProtocol.Mock) {
            bridgeFee = 0;
        } else {
            revert UnknownInteropProto(protoId);
        }
    }

    function _bridgeToValidated(BridgeToRequest memory req) internal {
        _quoteBridgeToValidated(req);
        _enforceSequentialNonce(req.from, req.bbNonce);
    }

    function _quoteBridgeToValidated(BridgeToRequest memory req) internal view {
        _ensureCheckpointNotCompleted(req.checkpointId);
        _revertIfZeroAddress(req.from);
        _revertIfZeroAddress(req.to);
        _revertIfZeroAddress(req.omToken);
        _revertIfZeroAmount(req.amount);
        _revertIfInvalidChain(req.dstChainId);
        _revertIfUnknownOmToken(req.omToken);
        _revertIfCheckpointComplete(req.checkpointId);
    }

    function _bridgeTo(BridgeToRequest memory req) internal bridgeToValidated(req) recordInboundNonce {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.TokenBinding storage binding = s.tokensByOm[req.omToken];
        uint256 refundAmount = _dispatchBridge(binding, req);

        _recordProcessedBridge(req.sourceHash, req.checkpointId);

        emit OMInteropSent(
            _latestInboundNonceInternal(), req.from, refundAmount, req.omToken, req.dstChainId, req.sourceHash
        );
    }

    function _recordProcessedBridge(bytes32 sourceHash, uint64 checkpointId) internal {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.CheckpointTally storage tally = s.checkpointTallies[checkpointId];

        s.processedBridges[sourceHash] = true;

        // If the checkpoint is known when recording the processed bridge, update the completed count
        if (checkpointId > 0) {
            s.checkpointIdsBySourceHash[sourceHash] = checkpointId;
            tally.completed += 1;
            _tryPruneCheckpoint(checkpointId);
        }
    }

    function _tryPruneCheckpoint(uint64 checkpointId) internal {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (!_isCheckpointComplete(checkpointId)) {
            return;
        }

        while (_isCheckpointComplete(s.earliestIncompletedCheckpoint)) {
            uint64 finished = s.earliestIncompletedCheckpoint;
            delete s.checkpointTallies[finished];
            unchecked {
                s.earliestIncompletedCheckpoint = finished + 1;
            }
        }
    }

    function _isCheckpointComplete(uint64 checkpointId) internal view returns (bool) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.CheckpointTally storage tally = s.checkpointTallies[checkpointId];
        return tally.certified != 0 && tally.completed >= tally.certified;
    }

    function _ensureCheckpointNotCompleted(uint64 checkpointId) internal view {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        if (checkpointId != 0 && checkpointId < s.earliestIncompletedCheckpoint) {
            revert CheckpointCompletedAndPruned(checkpointId);
        }
    }

    function _latestInboundNonceInternal() internal view returns (uint64) {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        uint64 next = s.nextInboundNonce;
        if (next == 0) {
            revert InboundNonceUnavailable();
        }
        unchecked {
            return next - 1;
        }
    }

    function _dispatchBridge(OMInteropTypes.TokenBinding storage binding, BridgeToRequest memory req)
        internal
        returns (uint256 refundAmount)
    {
        InteropProtocol protoId = binding.interopProtoId;
        if (protoId == InteropProtocol.LayerZero) {
            uint256 rawFee = _bridgeWithLayerZero(binding.scToken, req);
            uint256 paidFee = _computeFee(binding.omToken, rawFee);
            refundAmount = req.escrowFee >= paidFee ? req.escrowFee - paidFee : 0;
        } else if (protoId == InteropProtocol.Mock) {
            refundAmount = req.escrowFee;
        } else {
            revert UnknownInteropProto(protoId);
        }
    }

    function setRateLimit(address token, uint256 limit, uint256 window) external onlyOperator {
        _setRateLimit(token, limit, window);
    }

    function _setRateLimit(address token, uint256 limit, uint256 window) internal {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        // Clear rate limit if window is 0
        if (window == 0) {
            delete s.rateLimitsParam[token];
            delete s.rateLimitsData[token];
            emit RateLimitsChanged(token, limit, window);
            return;
        }

        OMInteropTypes.RateLimitParam storage rlParam = s.rateLimitsParam[token];
        OMInteropTypes.RateLimitData storage rlData = s.rateLimitsData[token];

        rlParam.limit = limit;
        rlParam.window = window;

        rlData.amountInFlight = 0;
        rlData.lastEpoch = block.timestamp / rlParam.window;
        emit RateLimitsChanged(token, limit, window);
    }

    function _checkAndUpdateRateLimit(address token, uint256 _amount) internal virtual {
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        OMInteropTypes.RateLimitParam storage rlParam = s.rateLimitsParam[token];
        OMInteropTypes.RateLimitData storage rlData = s.rateLimitsData[token];

        // A windows configured as 0 means no rate limiting
        if (rlParam.window == 0) return;

        uint256 currentEpoch = block.timestamp / rlParam.window;

        if (currentEpoch > rlData.lastEpoch) {
            rlData.amountInFlight = 0;
            rlData.lastEpoch = currentEpoch;
        }

        if (rlData.amountInFlight + _amount > rlParam.limit) {
            revert RateLimitExceeded();
        }

        rlData.amountInFlight += _amount;
    }

    function _computeFee(address token, uint256 paidFee) internal view returns (uint256 refundAmount) {
        // Compute refund based on price oracle
        OMInteropStorage.Layout storage s = OMInteropStorage.layout();
        address oracleAddr = s.priceOracle;
        (refundAmount,) = IPriceOracle(oracleAddr).convertNativeTokenToToken(token, paidFee);
    }
}
