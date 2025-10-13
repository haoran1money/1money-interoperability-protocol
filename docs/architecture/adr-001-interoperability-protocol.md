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

TODO: more context 

- payments, quorums, certificates
- requirements: fees in the transferred token, MM integrations, 
- governance, membership 
- cross-chain communication protocols, LZ 

![Internal Payments](./figures/adr-001-internal-payments.png)

## Decision

The EVM-compatible sidechain is a PoA blockchain that uses [Malachite](https://github.com/circlefin/malachite) 
for consensus and [Reth](https://github.com/paradigmxyz/reth) for execution. 
The consensus participants are the same as the 1Money Network validators, which makes the sidechain more like a sidecar.

For brevity, this ADR is using Ethereum as a external chain and LayerZero as a cross-chain communication protocol. 
However, the proposed design applies to any cross-chain communication protocol that satisfies the following conditions:

- It supports EVM-compatible chains. 
- It supports cross-chain token transfer. 
- The cross-chain token transfer functionality can be customized to remove any mint (un-escrow) and burn (escrow) logic.

Also, the external chain can be any chain supported by the cross-chain communication protocol. 

### External Incoming Payments 

The following diagram describes the high-level design of an external incoming payment, e.g., a user sending 100 USDT from 
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

#### Detailed Design 



### External Outgoing Payments 

The following diagram describes the high-level design of an external outgoing payment, e.g., a user sending 100 USDT from 
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

#### Detailed Design 




## Invariants

## References