// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/**
 * @title TxHashMapping * @notice The transaction hash mapping contract described in ADR-001.
 */
contract TxHashMapping is Ownable {
    // ---------- Errors ----------
    error Unauthorized();
    error InvalidHash();
    error InvalidAddress();
    error UnsetHash();
    error AlreadySet();
    error AlreadyLinked();
    error IncompleteWithdrawal();
    error ReverseAlreadyLinked();

    // ---------- Events ----------
    event RelayerUpdated(address indexed oldRelayer, address indexed newRelayer);
    event DepositRegistered(bytes32 indexed bridgeFromTxHash);
    event DepositHashesLinked(bytes32 indexed bridgeFromTxHash, bytes32 indexed bridgeAndMintTxHash);

    event WithdrawalRegistered(bytes32 indexed bridgeFromTxHash);
    event WithdrawalHashesLinked(bytes32 indexed bridgeFromTxHash, bytes32 indexed bridgeAndMintTxHash);
    event RefundHashesLinked(bytes32 indexed bridgeFromTxHash, bytes32 indexed refundTxHash);

    // ---------- Access ----------
    address public relayer;

    modifier onlyRelayer() {
        if (msg.sender != relayer) revert Unauthorized();
        _;
    }

    modifier nonZeroHash(bytes32 hash) {
        if (hash == bytes32(0)) revert InvalidHash();
        _;
    }

    modifier nonZeroAddress(address account) {
        if (account == address(0)) revert InvalidAddress();
        _;
    }

    // ---------- Storage ----------
    struct DepositEntry {
        bytes32 bridgeAndMintTxHash;
        bool exists;
    }

    struct WithdrawalEntry {
        bytes32 bridgeToTxHash;
        bytes32 refundTxHash;
        bool exists;
    }

    mapping(bytes32 => DepositEntry) private _depositMapping;
    mapping(bytes32 => bytes32) private _depositReverseMapping;

    bytes32[] private _incompleteDeposits;
    mapping(bytes32 => uint256) private _incompleteDepositIndex;

    mapping(bytes32 => WithdrawalEntry) private _withdrawalMapping;
    mapping(bytes32 => bytes32) private _burnAndBridgeFromBridgeToMapping;
    mapping(bytes32 => bytes32) private _burnAndBridgeFromRefundMapping;

    bytes32[] private _incompleteWithdrawals;
    bytes32[] private _incompleteRefunds;
    mapping(bytes32 => uint256) private _incompleteWithdrawalIndex;
    mapping(bytes32 => uint256) private _incompleteRefundIndex;

    // ---------- Constructor ----------
    constructor(address owner_, address relayer_) Ownable(owner_) nonZeroAddress(owner_) nonZeroAddress(relayer_) {
        relayer = relayer_;
        emit RelayerUpdated(address(0), relayer_);
    }

    // ---------- Admin ----------
    function setRelayer(address newRelayer) external onlyOwner nonZeroAddress(newRelayer) {
        emit RelayerUpdated(relayer, newRelayer);
        relayer = newRelayer;
    }

    // ---------- Core API ----------
    /// @notice Register a deposit tx hash.
    function registerDeposit(bytes32 bridgeFromTxHash) external onlyRelayer nonZeroHash(bridgeFromTxHash) {
        DepositEntry storage entry = _depositMapping[bridgeFromTxHash];
        if (entry.exists) revert AlreadySet();
        entry.exists = true;

        if (_incompleteDepositIndex[bridgeFromTxHash] == 0) {
            _incompleteDeposits.push(bridgeFromTxHash);
            _incompleteDepositIndex[bridgeFromTxHash] = _incompleteDeposits.length;
        }

        emit DepositRegistered(bridgeFromTxHash);
    }

    /// @notice Register a withdrawal tx hash.
    function registerWithdrawal(bytes32 burnAndBridgeTxHash) external onlyRelayer nonZeroHash(burnAndBridgeTxHash) {
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridgeTxHash];
        if (entry.exists) revert AlreadySet();
        entry.exists = true;

        if (_incompleteWithdrawalIndex[burnAndBridgeTxHash] == 0) {
            _incompleteWithdrawals.push(burnAndBridgeTxHash);
            _incompleteWithdrawalIndex[burnAndBridgeTxHash] = _incompleteWithdrawals.length;
        }
        if (_incompleteRefundIndex[burnAndBridgeTxHash] == 0) {
            _incompleteRefunds.push(burnAndBridgeTxHash);
            _incompleteRefundIndex[burnAndBridgeTxHash] = _incompleteRefunds.length;
        }

        emit WithdrawalRegistered(burnAndBridgeTxHash);
    }

    /// @notice Link BridgeFrom => BridgeAndMint tx hash.
    function linkDepositHashes(bytes32 bridgeFromTxHash, bytes32 bridgeAndMintTxHash)
        external
        onlyRelayer
        nonZeroHash(bridgeFromTxHash)
        nonZeroHash(bridgeAndMintTxHash)
    {
        DepositEntry storage entry = _depositMapping[bridgeFromTxHash];
        if (!entry.exists) revert UnsetHash();
        if (entry.bridgeAndMintTxHash != bytes32(0)) revert AlreadyLinked();

        bytes32 existingFrom = _depositReverseMapping[bridgeAndMintTxHash];
        if (existingFrom != bytes32(0) && existingFrom != bridgeFromTxHash) {
            revert ReverseAlreadyLinked();
        }

        entry.bridgeAndMintTxHash = bridgeAndMintTxHash;
        _depositReverseMapping[bridgeAndMintTxHash] = bridgeFromTxHash;

        uint256 idxPlus1 = _incompleteDepositIndex[bridgeFromTxHash];
        if (idxPlus1 != 0) {
            uint256 idx = idxPlus1 - 1;
            uint256 last = _incompleteDeposits.length - 1;
            if (idx != last) {
                bytes32 moved = _incompleteDeposits[last];
                _incompleteDeposits[idx] = moved;
                _incompleteDepositIndex[moved] = idx + 1;
            }
            _incompleteDeposits.pop();
            _incompleteDepositIndex[bridgeFromTxHash] = 0;
        }

        emit DepositHashesLinked(bridgeFromTxHash, bridgeAndMintTxHash);
    }

    /// @notice Link BurnAndBridge => BridgeTo tx hash.
    function linkWithdrawalHashes(bytes32 burnAndBridgeTxHash, bytes32 bridgeToTxHash)
        external
        onlyRelayer
        nonZeroHash(burnAndBridgeTxHash)
        nonZeroHash(bridgeToTxHash)
    {
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridgeTxHash];
        if (!entry.exists) revert UnsetHash();
        if (entry.bridgeToTxHash != bytes32(0)) revert AlreadyLinked();

        entry.bridgeToTxHash = bridgeToTxHash;
        _burnAndBridgeFromBridgeToMapping[bridgeToTxHash] = burnAndBridgeTxHash;

        uint256 idxPlus1 = _incompleteWithdrawalIndex[burnAndBridgeTxHash];
        if (idxPlus1 != 0) {
            uint256 idx = idxPlus1 - 1;
            uint256 last = _incompleteWithdrawals.length - 1;
            if (idx != last) {
                bytes32 moved = _incompleteWithdrawals[last];
                _incompleteWithdrawals[idx] = moved;
                _incompleteWithdrawalIndex[moved] = idx + 1;
            }
            _incompleteWithdrawals.pop();
            _incompleteWithdrawalIndex[burnAndBridgeTxHash] = 0;
        }

        emit WithdrawalHashesLinked(burnAndBridgeTxHash, bridgeToTxHash);
    }

    /// @notice Link BurnAndBridge => BridgeTo tx hash.
    function linkRefundHashes(bytes32 burnAndBridgeTxHash, bytes32 refundTxHash)
        external
        onlyRelayer
        nonZeroHash(burnAndBridgeTxHash)
        nonZeroHash(refundTxHash)
    {
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridgeTxHash];
        if (!entry.exists) revert UnsetHash();
        if (entry.refundTxHash != bytes32(0)) revert AlreadyLinked();

        entry.refundTxHash = refundTxHash;
        _burnAndBridgeFromRefundMapping[refundTxHash] = burnAndBridgeTxHash;

        uint256 idxPlus1 = _incompleteRefundIndex[burnAndBridgeTxHash];
        if (idxPlus1 != 0) {
            uint256 idx = idxPlus1 - 1;
            uint256 last = _incompleteRefunds.length - 1;
            if (idx != last) {
                bytes32 moved = _incompleteRefunds[last];
                _incompleteRefunds[idx] = moved;
                _incompleteRefundIndex[moved] = idx + 1;
            }
            _incompleteRefunds.pop();
            _incompleteRefundIndex[burnAndBridgeTxHash] = 0;
        }

        emit RefundHashesLinked(burnAndBridgeTxHash, refundTxHash);
    }

    // ---------- Views ----------
    function depositExists(bytes32 bridgeFromTxHash) external view returns (bool) {
        return _depositMapping[bridgeFromTxHash].exists;
    }

    function depositReverseExists(bytes32 bridgeAndMintTxHash) external view returns (bool) {
        return _depositReverseMapping[bridgeAndMintTxHash] != bytes32(0);
    }

    function withdrawalExists(bytes32 burnAndBridgeTxHash) external view returns (bool) {
        return _withdrawalMapping[burnAndBridgeTxHash].exists;
    }

    function getLinkedDeposit(bytes32 bridgeFromTxHash) external view returns (bytes32) {
        return _depositMapping[bridgeFromTxHash].bridgeAndMintTxHash;
    }

    function getLinkedWithdrawal(bytes32 burnAndBridgeTxHash) external view returns (bytes32) {
        return _withdrawalMapping[burnAndBridgeTxHash].bridgeToTxHash;
    }

    function getLinkedRefund(bytes32 burnAndBridgeTxHash) external view returns (bytes32) {
        return _withdrawalMapping[burnAndBridgeTxHash].refundTxHash;
    }

    /// @notice Combined getter that avoids sentinel checks in callers.
    function getDepositByBridgeFrom(bytes32 bridgeFromTxHash) external view returns (bytes32 linked, bool isSet) {
        DepositEntry storage entry = _depositMapping[bridgeFromTxHash];
        return (entry.bridgeAndMintTxHash, entry.exists);
    }

    /// @notice Look up bridgeFrom by bridgeAndMint; returns 0x0 if unknown.
    function getDepositByBridgeAndMint(bytes32 bridgeAndMintTxHash) external view returns (bytes32 bridgeFromTxHash) {
        return _depositReverseMapping[bridgeAndMintTxHash];
    }

    /// @notice Combined getter that avoids sentinel checks in callers.
    function getWithdrawal(bytes32 burnAndBridgeTxHash)
        external
        view
        returns (bytes32 bridgeTo, bytes32 refund, bool isSet)
    {
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridgeTxHash];
        return (entry.bridgeToTxHash, entry.refundTxHash, entry.exists);
    }

    /// @notice Combined getter that avoids sentinel checks in callers.
    function getWithdrawalFromBridgeTo(bytes32 bridgeToTxHash)
        external
        view
        returns (bytes32 burnAndBridge, bytes32 refund, bool isSet)
    {
        burnAndBridge = _burnAndBridgeFromBridgeToMapping[bridgeToTxHash];
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridge];
        return (burnAndBridge, entry.refundTxHash, entry.exists);
    }

    /// @notice Combined getter that avoids sentinel checks in callers.
    function getWithdrawalFromRefund(bytes32 refundTxHash)
        external
        view
        returns (bytes32 burnAndBridge, bytes32 bridgeTo, bool isSet)
    {
        burnAndBridge = _burnAndBridgeFromRefundMapping[refundTxHash];
        WithdrawalEntry storage entry = _withdrawalMapping[burnAndBridge];
        return (burnAndBridge, entry.bridgeToTxHash, entry.exists);
    }

    /// @notice Page through all registered keys for deposits.
    function incompleteDeposits() external view returns (bytes32[] memory) {
        return _incompleteDeposits;
    }

    /// @notice Page through all registered keys for withdrawals.
    function incompleteWithdrawals() external view returns (bytes32[] memory) {
        return _incompleteWithdrawals;
    }

    /// @notice Page through all registered keys for refunds.
    function incompleteRefunds() external view returns (bytes32[] memory) {
        return _incompleteRefunds;
    }
}
