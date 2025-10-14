# 1Money Interoperability Protocol

## Changelog

* 2025-10-13: Initial draft

## Status

PROPOSED: Not Implemented

## Abstract

This ADR describes the design of a protocol that enables the interoperability between the 1Money Network 
and existing networks, such as Ethereum, via third-party cross-chain communication protocols, 
such as LayerZero or Wormhole. 

## Context

The 1Money Network consists of a fast payment lane that requires Byzantine Consistent Broadcast (BCB) 
and a checkpointing protocol that enables the network participants (i.e., validators) to reach agreement 
on the system state. Although the checkpointing protocol implements Byzantine consensus, it is not 
EVM-compatible, which means that it doesn't support out-of-the-box integration with cross-chain communication 
protocols, such as LayerZero or Wormhole. As a result, this ADR is proposing the addition of an EVM-compatible 
sidechain, that supports integration with cross-chain communication protocols. We refer to this as the EVM lane. 
In addition, this ADR is proposing the design of the interoperability protocol that enables the communication 
between the payment lane and the EVM lane. 

> TODO: add more context 
>
> - payments, quorums, certificates
> - requirements: fees in the transferred token, MM integrations, 
> - governance, membership 
> - cross-chain communication protocols, LZ 

![Internal Payments](./figures/adr-001-internal-payments.png)

> TODO: move steps from diagram to doc

## Decision

The EVM-compatible sidechain is a PoA blockchain that uses [Malachite](https://github.com/circlefin/malachite) 
for consensus and [Reth](https://github.com/paradigmxyz/reth) for execution. 
The consensus participants are the same as the 1Money Network validators, which makes the sidechain more like a sidecar.

For brevity, this ADR is using Ethereum as a external chain and LayerZero as a cross-chain communication protocol. 
However, the proposed design applies to any cross-chain communication protocol that satisfies the following conditions:

- It supports EVM-compatible chains. 
- It supports cross-chain token transfer. 
- The cross-chain token transfer functionality can be customized to remove any mint (unlock) and burn (lock) logic.

Also, the external chain can be any chain supported by the cross-chain communication protocol. 

Cross-chain communication protocols deploy their logic in the form of EVM smart contracts on the sidechain. 

### High Level Design

> TODO improve diagrams and description to be consistent with the rest

<details>
<summary>High level design of external payments (i.e., cross-chain payments).</summary>

**External Incoming Payments.** The following diagram describes the high-level design of an external incoming payment, e.g., a user sending 100 USDT from 
Ethereum to the 1Money Network. Here is the step by step flow:

- The user logs in on the 1Money (1MN) frontend with Metamask (MM) wallet. 
- The user inputs intent on 1MN frontend (e.g., send 100 USDT from Ethereum to 1MN).
- 1MN frontend queries Ethereum for LayerZero (LZ) fee (by calling `quote()`). The fee is in ETH or ZRO tokens.
- The user signs transaction using MM wallet.
- 1MN frontend sends signed transaction -- LZ `send()` -- to Ethereum.
- LZ triggers `receive()` on 1MN Sidechain. LZ operators need to have the _1GT token_ (the 1MN Sidechain gas token).
- 1MN Sidechain handles `receive()` by just emitting an event (no mint).
- The Sidechain event acts as a `mintToForBridge()` for the 1MN L1 (i.e., the payment network).
- 1MN L1 creates certificate for `mintToForBridge` and updates account balances.
- 1MN frontend queries balances endpoint to update local view.

![External Incoming Payments](./figures/adr-001-external-incoming-payments.png)

**External Outgoing Payments.** The following diagram describes the high-level design of an external outgoing payment, e.g., a user sending 100 USDT from 
the 1Money Network to Ethereum. Here is the step by step flow:

- The user logs in on 1MN frontend with MM wallet. 
- The user inputs intent on 1MN frontend (e.g., send 100 USDT from 1MN to Ethereum).
- 1MN frontend queries 1MN L1 for fee estimate including the LZ fee. The fee is in USDT.
- The user signs transaction using MM wallet.
- 1MN frontend sends signed transaction -- `burnAndBridge()` -- to 1MN L1.
- 1MN L1 creates certificate for `burnAndBridge` and updates account balances (burn transfer amount and escrow fees).
- A relayer sends a `bridgeTo()` transaction (including the certificate) to the Sidechain. This transaction calls LZ `send()`. 
- LZ triggers `receive()` on Ethereum. 
- Once LZ transfer is finalize (on `bridgeTo` success), the relayer sends a `collectFees` transaction to 1MN L1. 
- 1MN L1 creates certificate for `collectFees` and escrowed fees are sent to the relayer. 
- 1MN frontend queries balances endpoint to update local view. 

![External Outgoing Payments](./figures/adr-001-external-outgoing-payments.png)
</details>

### Main Components

The following diagram describes the components necessary to enable interoperability with external chains. 
The components in red are dependencies, while the ones in orange are within the scope of this ADR. 

![Interop Main Components](./figures/adr-001-main-components.png)

- **Customized Token Transfer Contracts.** These are customized contracts that replace mint (unlock) and burn (lock) 
  logic with calls into the _1Money Interop Contract_ (see below).
- **1Money Interop Contract.** A contract (i.e., `OMInterop.sol`) that acts as an interface between the 1Money Network 
  and third-party cross-chain protocols, such as LayerZero or Wormhole. This contract contains the logic that enables 
  the _Permissioned Relayer_ (see below) to translate events from the sidechain to actions on the payment network.
- **Permissioned Relayer.** An off-chain _permissioned_ relayer that translates events on both sidechain and payment network 
  into actions on the other side. 
- **Interop Module.** A module on the 1Money payment network that contains logic to mint incoming cross-chain tokens 
  and to burn outgoing cross-chain tokens. 

### Interop Module

The Interop Module introduces three new instructions to the payment network: `MintToForBridge`, `BurnAndBridge`, and `CollectFees`

#### MintToForBridge

Creates new cross-chain tokens and adds them to a specified account. 

Parameters:

- `amount: U256` -- The amount of tokens to mint
- `address: Address` -- The recipient's wallet address

Required Permissions:

- The transaction signer must be the Permissioned Relayer. 
- The token must not be paused.

> TBD: Can the recipient be blacklisted? 
  
Notes:

- Minting increases the token's total supply.
- The recipient's token account is created automatically if it doesn't exist.
- To ensure subsequent nonces, the nonce is determined by the sidechain. 
- Before a cross-chain token can be minted, it first needs to be created via the `CreateNewToken` instruction. The 
  `master_authority` is set to the Permissioned Relayer.

> TBD: Is thea solution based on a permissioned relayer acceptable? Should the `master_authority` be someone else and 
> make the relayer a `mint_burn_authority`? 

#### BurnAndBridge

Destroys cross-chain tokens by removing them from a specified account and transferring them to another account on 
a destination chain. 

Parameters:

- `amount: U256` -- The amount of tokens to burn and bridge
- `address: Address` -- The recipient's wallet address
- `dstChainId: U32` -- The destination chain ID
- `escrowFee: U256` -- The bridging fee necessary to escrow for transferring the tokens to the destination chain

> TBD: In LayerZero, a destination endpoint ID is a [uint32](https://github.com/LayerZero-Labs/LayerZero-v2/blob/200cda254120375f40ed0a7e89931afb897b8891/packages/layerzero-v2/evm/oapp/contracts/oft/interfaces/IOFT.sol#L11). 
> In Wormhole, the recipientChain is a [uint16](https://github.com/wormhole-foundation/native-token-transfers/blob/main/evm/src/interfaces/INttManager.sol#L175). 
> Keep `uint32` for now, but how to make this compatible with other interop protocols? 

> TBD: Do we want the user to have an option to choose the cross-chain communication protocol? 

Required Permissions:

- The transaction signer must be the owner of the source token account
- The token must not be paused
- The sender cannot be blacklisted

Notes:

- Transfers fail if the sender has insufficient balance
- Burning decreases the token's total supply
- The bridging fee is escrowed in a special account owned by the Permissioned Relayer.
- The `dstChainId` is specific to the cross-chain communication protocol and is public information 
  (see [here](https://wormhole.com/docs/products/reference/chain-ids/) for Wormhole).

> TBD: In the [Transfer](https://developer.1moneynetwork.com/core-concepts/transactions-and-instructions#transfer) 
> instruction, is the amount the amount received by the recipient or sent by the sender?
> In other words, is the payment fee part of the amount?

#### CollectFees

Transfer fees for cross-chain transfers from the special escrow account. 

Parameters:

- `fee: U256` -- The actual amount paid by the relayer in fees for a cross-chain transfer
- `feeAddress: Address` -- The recipient's wallet address for the fee amount
- `refund: U256` -- The amount to be refunded to the user (i.e., `escrowFee - fee`)
- `refundAddress: Address` -- The recipient's wallet address for the refund amount

Required Permissions:

- The transaction signer must be the Permissioned Relayer. 
- The refund address cannot be blacklisted

### Permissioned Relayer 

> TODO

- needs mapping from token addresses on sidechain to token addresses on payment network 
- needs to be able to query certified `BurnAndBridge` instructions
- needs to be able to send payment network instructions (`MintToForBridge` and `CollectFees`) with subsequent nonces 
- needs to be able to consume events emitted by the sidechain in order (to keep the nonces subsequent)
- needs to be able to eventually retrieve all certified `BurnAndBridge` instructions
- needs enough information to be able to send `CollectFees` instructions

### 1Money Interop Contract

The `OMInterop.sol` contract acts as an interface between the 1Money Network and third-party cross-chain protocols, 
such as LayerZero or Wormhole. Specifically, the contract enables the Permissioned Relayer to listen to cross-chain 
events and to trigger cross-chain actions on the sidechain.

At a minimum, `OMInterop.sol` should define two events and two external functions:
- `event OMInteropReceived` emitted when cross-chain tokens are received on the sidechain, i.e., the `receive()` call 
  of the corresponding cross-chain protocol was successful.
- `event OMInteropSent` emitted when cross-chain tokens are sent on the sidechain, i.e., the `send()` call 
  of the corresponding cross-chain protocol was successful.
- `function bridgeFrom` called by the Customized Token Transfer Contracts instead of mint. The function emits `OMInteropReceived`.
- `function bridgeTo` called by the Permissioned Relayer. The function calls the `send()` function of the corresponding 
  cross-chain token transfer protocol and emits `OMInteropSent`. 

  ```solidity
    interface IOMInterop {
        // Events
        event OMInteropReceived(
            address to, // Destination account
            uint256 amount, // Amount of tokens to mint
            address omToken // The token address on the 1Money payment network
        );
        event OMInteropSent(
            address from, // Source account (needed to refund the unused fee)
            uint256 feeAmount, // Amount of tokens to transfer to the relayer
            uint256 refundAmount, // Amount of tokens to refund the user (refundAmount = escrowFee - feeAmount)
            address omToken // The token address on the 1Money payment network
        )
        
        // mintForBridge emits OMInteropReceived
        function bridgeFrom(
            address to,
            uint256 amount
        ) external;

        // bridgeTo first calls the send() method of the corresponding 
        // cross-chain token transfer protocol 
        // and then emits OMInteropSent
        function bridgeTo(
            address from,
            address to,
            uint256 amount,
            uint32 dstChainId,
            uint256 escrowFee,
            address omToken
        ) external;
    }
  ```

`OMInterop.sol` needs to keep a mapping of token addresses between the 1Money payment network and the sidechain. 
This mapping is populated by the 1Money Network Operator that is responsible for deploying Customized Token Transfer 
Contracts on the sidechain and to submit corresponding CreateNewToken instructions to the payment network. 

> TBD: Does `bridgeTo()` need the interopProtoID as an argument or it can figure it out from the destination chain? 

> TODO: discuss nonces needed for the payment network 

### Customized Token Transfer Contracts

In the case of LayerZero, the [OFT.sol](https://github.com/LayerZero-Labs/LayerZero-v2/blob/main/packages/layerzero-v2/evm/oapp/contracts/oft/OFT.sol) 
contract needs to be extended to remove the `_burn()` and `_mint()` calls from `_debit()` and `_credit`, respectively.
The `_mint(_to, _amount)` should be replaced by a call to the `bridgeFrom(_to, _amount)` function of the `OMInterop.sol` contract.
An instance of the extended `OFT.sol` contract (i.e., `OM-OFT.sol`) will be deployed on the sidechain for every supported cross-chain token.

In the case of Wormhole, the [NttManager.sol](https://github.com/wormhole-foundation/native-token-transfers/blob/main/evm/src/NttManager/NttManager.sol)
contract needs to be extended. Specifically, the `_handleMsg` needs to be overridden to remove any mint (unlock) and burn (lock) logic. 
Similarly to LayerZero, the mint calls should be replaced by a call to the `bridgeFrom()` function of the `OMInterop.sol` contract.

### Edge Cases 

- relayer crashes --> needs to recover without missing events 
- mint fails --> is it possible? 
- bridgeTo fails --> is it possible? 
- 

## Invariants

- The relayer doesn't skip events as the payments need subsequent nonces. 
- Every burn on the payment network will eventually result in a matching send on the sidechain. 
- Every cross-chain token received on the sidechain will eventually result in a matching mint on the payment network. 

## References