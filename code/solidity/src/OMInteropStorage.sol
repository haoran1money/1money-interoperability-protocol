// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {OMInteropTypes} from "./OMInteropTypes.sol";

library OMInteropStorage {
    // Never change this string after deploy; it fixes the storage location.
    bytes32 internal constant STORAGE_LOCATION = keccak256("om.interop.storage.OMInterop.v1");

    /**
     * @custom:storage-location erc7201:om.interop.storage.OMInterop.v1
     */
    struct Layout {
        address operator;
        address relayer;
        uint64 version;
        uint64 nextInboundNonce;
        uint64 earliestIncompletedCheckpoint;
        // Mappings holding your existing structs
        mapping(address => OMInteropTypes.TokenBinding) tokensByOm;
        mapping(address => OMInteropTypes.TokenBinding) tokensBySidechain;
        mapping(address => uint64) latestBbNonce;
        mapping(uint64 => OMInteropTypes.CheckpointTally) checkpointTallies;
        mapping(address => OMInteropTypes.RateLimitParam) rateLimitsParam;
        mapping(address => OMInteropTypes.RateLimitData) rateLimitsData;
    }

    function layout() internal pure returns (Layout storage l) {
        bytes32 slot = STORAGE_LOCATION;
        assembly {
            l.slot := slot
        }
    }
}
