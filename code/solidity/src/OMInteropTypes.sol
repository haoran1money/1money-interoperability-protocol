// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {InteropProtocol} from "./IOMInterop.sol";

library OMInteropTypes {
    struct TokenBinding {
        address omToken;
        address scToken;
        InteropProtocol interopProtoId;
        bool exists;
    }

    struct CheckpointTally {
        uint32 certified;
        uint32 completed;
    }

    struct RateLimitParam {
        uint256 limit;
        uint256 window;
    }

    struct RateLimitData {
        uint256 amountInFlight;
        uint256 lastEpoch;
    }
}
