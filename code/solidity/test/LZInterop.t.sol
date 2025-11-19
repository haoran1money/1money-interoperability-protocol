// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.22;

import {OMInterop} from "../src/OMInterop.sol";
import {LZInterop} from "../src/LZInterop.sol";
import {IOMInterop, InteropProtocol} from "../src/IOMInterop.sol";
import {OMOFT} from "../src/OMOFT.sol";
import {OFT} from "@layerzerolabs/oft-evm/contracts/OFT.sol";
import {OptionsBuilder} from "@layerzerolabs/oapp-evm/contracts/oapp/libs/OptionsBuilder.sol";
import {MessagingFee, SendParam} from "@layerzerolabs/oft-evm/contracts/interfaces/IOFT.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {Vm} from "forge-std/Vm.sol";

import {TransparentUpgradeableProxy} from "@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

import {TestHelperOz5} from "@layerzerolabs/test-devtools-evm-foundry/contracts/TestHelperOz5.sol";
import {SimpleMessageLibMock} from "@layerzerolabs/test-devtools-evm-foundry/contracts/mocks/SimpleMessageLibMock.sol";

contract PlainOFT is Ownable, OFT {
    constructor(string memory name_, string memory symbol_, address endpoint_, address delegate_)
        Ownable(delegate_)
        OFT(name_, symbol_, endpoint_, delegate_)
    {}

    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }
}

contract MockLzToken is ERC20 {
    constructor() ERC20("MockLZ", "MLZ") {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract LZInteropTest is TestHelperOz5 {
    uint32 private constant LOCAL_EID = 1;
    uint32 private constant REMOTE_EID = 2;
    uint256 private constant BRIDGE_AMOUNT = 1e18;
    uint256 private constant MOCK_LZ_TOKEN_FEE = 5e15;

    address internal constant OWNER = address(0xA11CE);
    address internal constant OPERATOR = address(0xB0B);
    address internal constant RELAYER = address(0xC0FFEE);
    address internal constant OM_TOKEN = address(0xBEEF);

    bytes32 internal constant BURN_AND_BRIDGE_HASH = keccak256("burnandbridgeTxHash");

    OMInterop internal interop;
    OMOFT internal localOft;
    PlainOFT internal remoteOft;
    address internal sidechainToken;
    address internal proxyAdmin;
    MockLzToken internal lzToken;

    function setUp() public virtual override {
        super.setUp();

        setUpEndpoints(2, LibraryType.SimpleMessageLib);

        lzToken = new MockLzToken();

        for (uint256 i = 0; i < endpointSetup.endpointList.length; i++) {
            endpointSetup.endpointList[i].setLzToken(address(lzToken));
            SimpleMessageLibMock(payable(endpointSetup.sendLibs[i])).setMessagingFee(0, MOCK_LZ_TOKEN_FEE);
        }

        OMInterop impl = new OMInterop();

        bytes memory initData = abi.encodeCall(OMInterop.initialize, (OWNER, OPERATOR, RELAYER));

        // Deploy proxy
        ERC1967Proxy proxy = new ERC1967Proxy(address(impl), initData);

        // Cast proxy to the OMInterop type
        interop = OMInterop(address(proxy));

        proxyAdmin = makeAddr("proxyAdmin");

        localOft = _deployOmoftProxy("MockOMToken", "MOMT", endpoints[LOCAL_EID], address(interop), address(this));
        remoteOft = new PlainOFT("RemoteMockOMToken", "RMOMT", endpoints[REMOTE_EID], address(this));

        address[] memory ofts = new address[](2);
        ofts[0] = address(localOft);
        ofts[1] = address(remoteOft);
        wireOApps(ofts);

        sidechainToken = address(localOft);

        vm.prank(OPERATOR);
        interop.mapTokenAddresses(OM_TOKEN, sidechainToken, InteropProtocol.LayerZero);

        lzToken.mint(RELAYER, 1e21);
    }

    function _deployOmoftProxy(
        string memory name_,
        string memory symbol_,
        address endpoint_,
        address omInterop_,
        address delegate_
    ) internal returns (OMOFT) {
        bytes memory initData = abi.encodeWithSelector(OMOFT.initialize.selector, name_, symbol_, omInterop_, delegate_);

        bytes memory bytecode = bytes.concat(type(OMOFT).creationCode, abi.encode(endpoint_));

        address implementation;
        assembly {
            implementation := create(0, add(bytecode, 0x20), mload(bytecode))
            if iszero(extcodesize(implementation)) { revert(0, 0) }
        }

        return OMOFT(address(new TransparentUpgradeableProxy(implementation, proxyAdmin, initData)));
    }

    function testRemoteOftSendDeliversToSidechain() public {
        address remoteUser = address(0xA5);
        address localRecipient = address(0xB6);
        uint256 amountToSend = BRIDGE_AMOUNT;

        vm.expectEmit(true, true, false, true, address(remoteOft));
        emit IERC20.Transfer(address(0), remoteUser, amountToSend);
        remoteOft.mint(remoteUser, amountToSend);
        vm.deal(remoteUser, 1 ether);

        bytes memory options = OptionsBuilder.newOptions();
        options = OptionsBuilder.addExecutorLzReceiveOption(options, 200_000, 0);

        SendParam memory sendParam = SendParam({
            dstEid: LOCAL_EID,
            to: addressToBytes32(localRecipient),
            amountLD: amountToSend,
            minAmountLD: amountToSend,
            extraOptions: options,
            composeMsg: bytes(""),
            oftCmd: bytes("")
        });

        MessagingFee memory fee = remoteOft.quoteSend(sendParam, false);

        assertEq(remoteOft.balanceOf(remoteUser), amountToSend, "remote user balance should be funded");

        vm.prank(remoteUser);
        remoteOft.send{value: fee.nativeFee}(sendParam, fee, remoteUser);

        assertEq(remoteOft.balanceOf(remoteUser), 0, "remote user balance should be burned");

        assertEq(localOft.balanceOf(localRecipient), 0, "local recipient balance starts zero");
        assertEq(interop.getLatestInboundNonce(), 0, "inbound nonce should start at zero");

        vm.expectCall(address(interop), abi.encodeCall(IOMInterop.bridgeFrom, (localRecipient, amountToSend)));

        vm.recordLogs();
        verifyPackets(LOCAL_EID, addressToBytes32(address(localOft)));

        // manually inspect event logs due to issues in expectEmit with cross call and multiple events.
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length > 1, "expected OMInteropReceived log");
        Vm.Log memory entry = logs[1]; // second log is from OMInterop

        bytes32 expectedSig = keccak256("OMInteropReceived(uint64,address,uint256,address,uint32)");
        assertEq(entry.topics[0], expectedSig, "event sig mismatch");
        assertEq(entry.emitter, address(interop), "emitter mismatch");
        assertEq(entry.topics.length, 3, "topics length mismatch");

        address recipient = address(uint160(uint256(entry.topics[1])));
        address omToken = address(uint160(uint256(entry.topics[2])));
        (uint64 nonce, uint256 amount, uint32 srcChainId) = abi.decode(entry.data, (uint64, uint256, uint32));

        assertEq(recipient, localRecipient, "recipient mismatch");
        assertEq(omToken, OM_TOKEN, "token mismatch");
        assertEq(nonce, 0, "nonce mismatch");
        assertEq(amount, amountToSend, "amount mismatch");
        assertEq(srcChainId, LOCAL_EID, "src chain mismatch");

        assertEq(localOft.balanceOf(localRecipient), 0, "OMOFT keeps accounting off-chain");
        assertEq(interop.getLatestInboundNonce(), 1, "inbound nonce should advance");
    }

    function testSidechainDeliversToRemoteOftRecipient() public {
        uint64 checkpointId = 7;

        vm.prank(RELAYER);
        interop.updateCheckpointInfo(checkpointId, 1);

        address remoteRecipient = address(0xCAFEBABE);
        uint256 amountToSend = BRIDGE_AMOUNT;
        uint256 minAmountLd = amountToSend;
        uint128 maxGas = 200_000;
        bytes memory bridgeData = abi.encode(maxGas, minAmountLd);

        // Invalid bridgeData should revert with custom error before fee checks.
        vm.expectRevert(LZInterop.InvalidBridgeData.selector);
        vm.prank(RELAYER);
        interop.bridgeTo(
            address(0xAA),
            0,
            remoteRecipient,
            amountToSend,
            REMOTE_EID,
            0,
            OM_TOKEN,
            checkpointId,
            "",
            BURN_AND_BRIDGE_HASH
        );

        assertEq(remoteOft.balanceOf(remoteRecipient), 0, "remote recipient should start with zero balance");

        (uint256 bridgeFee, address feeToken) = interop.quoteBridgeTo(
            address(0xAA),
            0,
            remoteRecipient,
            amountToSend,
            REMOTE_EID,
            0,
            OM_TOKEN,
            checkpointId,
            bridgeData,
            BURN_AND_BRIDGE_HASH
        );
        assertEq(feeToken, address(lzToken), "bridge fee token should be ZRO");
        assertGt(bridgeFee, 0, "bridge fee should use ZRO tokens");

        vm.prank(RELAYER);
        lzToken.approve(address(interop), type(uint256).max);

        uint256 relayerLzBefore = lzToken.balanceOf(RELAYER);
        uint256 omLzBefore = lzToken.balanceOf(address(interop));
        vm.recordLogs();
        vm.prank(RELAYER);
        interop.bridgeTo(
            address(0xAA),
            0,
            remoteRecipient,
            amountToSend,
            REMOTE_EID,
            bridgeFee * 2,
            OM_TOKEN,
            checkpointId,
            bridgeData,
            BURN_AND_BRIDGE_HASH
        );
        uint256 relayerLzAfter = lzToken.balanceOf(RELAYER);
        assertEq(relayerLzBefore - relayerLzAfter, MOCK_LZ_TOKEN_FEE, "relayer should fund LZ fee");
        uint256 omLzAfter = lzToken.balanceOf(address(interop));
        assertEq(omLzAfter, omLzBefore, "OMInterop should not retain LZ tokens");
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length > 6, "expected OMInteropSent log");
        Vm.Log memory entry = logs[6]; // seventh log is OMInteropSent
        bytes32 sentSig = keccak256("OMInteropSent(uint64,address,uint256,address,uint32,bytes32)");
        assertEq(entry.emitter, address(interop), "OMInteropSent emitter mismatch");
        assertEq(entry.topics.length, 3, "OMInteropSent topics length mismatch");
        assertEq(entry.topics[0], sentSig, "log 6 should be OMInteropSent");
        address from = address(uint160(uint256(entry.topics[1])));
        address omToken = address(uint160(uint256(entry.topics[2])));
        assertEq(from, address(0xAA), "OMInteropSent from mismatch");
        assertEq(omToken, OM_TOKEN, "OMInteropSent token mismatch");
        (uint64 nonce_, uint256 refundAmount, uint32 dstChainId) = abi.decode(entry.data, (uint64, uint256, uint32));
        assertEq(dstChainId, REMOTE_EID, "OMInteropSent chain mismatch");
        assertEq(refundAmount, 0, "refund should be zero for LayerZero");
        uint64 latestNonce = interop.getLatestInboundNonce();
        assertEq(nonce_ + 1, latestNonce, "event nonce should match latest inbound");

        verifyPackets(REMOTE_EID, addressToBytes32(address(remoteOft)));

        assertEq(remoteOft.balanceOf(remoteRecipient), amountToSend, "bridgeTo should credit remote recipient");
    }
}
