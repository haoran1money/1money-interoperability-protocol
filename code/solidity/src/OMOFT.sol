// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.22;

import {OFTUpgradeable} from "@layerzerolabs/oft-evm-upgradeable/contracts/oft/OFTUpgradeable.sol";
import {IOMInterop} from "./IOMInterop.sol";

contract OMOFT is OFTUpgradeable {
    error OnlyOmInterop();

    address private omInterop;

    constructor(address lzEndpoint_) OFTUpgradeable(lzEndpoint_) {
        _disableInitializers();
    }

    function initialize(string memory name_, string memory symbol_, address omInterop_, address delegate_)
        public
        initializer
    {
        omInterop = omInterop_;
        __OFT_init(name_, symbol_, delegate_);
        __Ownable_init(delegate_);
    }

    modifier onlyOmInterop() {
        _onlyOmInterop();
        _;
    }

    function _onlyOmInterop() internal view {
        if (msg.sender != omInterop) revert OnlyOmInterop();
    }

    function _debit(
        address,
        /* _from */
        uint256 amountLd,
        uint256 minAmountLd,
        uint32 dstEid
    ) internal virtual override onlyOmInterop returns (uint256 amountSentLd, uint256 amountReceivedLd) {
        (amountSentLd, amountReceivedLd) = _debitView(amountLd, minAmountLd, dstEid);
    }

    function _credit(address to, uint256 amountLd, uint32 /*srcEid*/ )
        internal
        virtual
        override
        returns (uint256 amountReceivedLd)
    {
        amountReceivedLd = amountLd;
        IOMInterop(omInterop).bridgeFrom(to, amountLd);
    }
}
