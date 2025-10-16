# 1Money Proof of Authority 

## Changelog

* 2025-10-16: Initial draft

## Status

PROPOSED: Not Implemented

## Abstract

This ADR describes the design of a protocol that enables the 1Money sidechain to run as a Proof of Authority (PoA)
network with the same validator set as the 1Money payment network (aka the 1Money L1).

## Context

The 1Money L1 is a PoA network -- the network participants are a set of permissioned validators. 
Membership changes, i.e., adding or removing permissioned validators, occur via the governance system. 
Membership changes are initiated by the 1Money Network Operator. 

* The operator creates a governance proposal with all the changes. 
* The operator gets signatures from the current validators off-chain. 
  The reason this is done off-chain (instead of using the existing broadcast network) is for security reasons. 
* The operator broadcast the certified governance proposal to the network.
* Once a validator sees a certified governance proposal, it triggers an epoch change, 
  i.e., a new checkpoint that increments the epoch ID. Once the checkpoint completes,
  the validator starts using the new validator set. 

The 1Money L1 validator set can be queried via two REST APIs:

* `/v1/governances/epoch` -- returns the current epoch and its validator set
* `/v1/governances/epoch/by_id` -- returns a specific epoch based on the epoch ID

## Decision

The EVM-compatible sidechain is a PoA blockchain that uses [Malachite](https://github.com/circlefin/malachite) 
for consensus and [Reth](https://github.com/paradigmxyz/reth) for execution. 
The consensus participants must be the same as the 1Money L1 validators. 
This ADR describes the protocol that enables the sidechain to have the same validator set as the 1Money L1. 
The protocol consists of two components:

- A PoA contract that is deployed on the sidechain. 
  The contract enables an owner, i.e., the permissioned relayer (see below), 
  to add and remove validators to the sidechain's validator set. 
- A permissioned relayer (i.e., the same as the one described in [ADR-001](./adr-001-interoperability-protocol.md)), 
  that listens to membership changes on the 1Money L1 and applies these changes to the sidechain by calling into 
  the PoA contract.

### PoA Contract

At a minimum, `OMPoA.sol` should define the following events and external functions:

- `event OMValidatorAdded` emitted when a new validator is added
- `event OMValidatorRemoved` emitted when an existing validator is removed
- `function addValidator` called by the Permissioned Relayer to add a new validator
- `function removeValidator` called by the Permissioned Relayer to remove a new validator
- `function addAndRemoveValidators` a utility function to avoid multiple calls to `addValidator` and `removeValidator`.

`OMPoA.sol` stores a list of the current validators. This list is provided to the consensus engine (i.e., Malachite), 
which uses it to update its validator set. Not that every validator in the set gets the same voting power. 

### Permissioned Relayer

The Permissioned Relayer must listen to membership changes on the 1Money L1, 
using the `/v1/governances/epoch` and `/v1/governances/epoch/by_id` REST APIs. 
When it detects a change in membership, it submits a `addAndRemoveValidators` transaction to the sidechain.
For this, it needs to have gas tokens. 

## Consequences

* The validator set on the sidechain will be slightly out of sync with the validator set on the payment network.
  In other words, membership changes on the payment network will be adapted by the sidechain with a delay. 
  This has no impact on the system due to the assumption of a trusted Permissioned Relayer that monitors both 
  networks and translates events from each side into action on the other side 
  (see [ADR-001](./adr-001-interoperability-protocol.md)). 
  As the relayer is trusted, the validation of transactions submitted by the relayer doesn't entail checking signatures 
  of the validator set. 
  Note that the assumption of a trusted Permissioned Relayer is only needed for the current design 
  where the payment network and the sidechain have separated states. 
  In future iterations, when the payment network and the sidechain will share a common state, the protocol 
  should be adapted to work without this assumption.

## References

* [Malachite](https://github.com/circlefin/malachite) 
* [Reth](https://github.com/paradigmxyz/reth)
* [ADR-001 1Money Interoperability Protocol](./adr-001-interoperability-protocol.md)