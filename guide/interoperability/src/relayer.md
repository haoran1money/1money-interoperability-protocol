# Relayer

There are four commands which can be used to run the relayer, one for each feature and one to run them all together:

* `relayer proof-of-authority` will run the process which handles Proof-of-Authority events from 1Money to the sidechain
* `relayer sidechain` will handles transaction from the Sidechain to 1Money
* `relayer onemoney` will handle transaction from 1Money to the Sidechain
* `relayer all` will run the three processes above concurrently

> Note: To run the relayer it is required to know the contract addresses of the 1Money interoperability and Tx Hash Mapping contracts, as well as the relayer's private key.

## Configuration

The commands has multiple flags, this section will focus on the behaviour when running with the required flags only.

The endpoint addresses are set by using environment variables:

* `OM_NODE_URL`: URL of the 1Money node to connect to
* `SC_HTTP_URL`: HTTP URL of the sidechain node to connect to
* `SC_WS_URL`: WebSocket URL of the sidechain node to connect to

These values will default to the following if not set:

```
OM_NODE_URL=http://127.0.0.1:18555
SC_HTTP_URL=http://127.0.0.1:8645
SC_WS_URL=http://127.0.0.1:8646
```

## Sidechain

`relayer --interop-contract-address <INTEROP_CONTRACT_ADDRESS> --tx-mapping-contract-address <TX_MAPPING_CONTRACT_ADDRESS> --relayer-private-key <RELAYER_PRIVATE_KEY> sidechain`

This will start the relayer which will do the following steps in this order:

1. Verify if there are incomplete hash mappings for the deposits and complete them if possible
2. Verify if there are pending deposits and complete them if there are
3. Start listening to Sidechain events and process them

## Onemoney

`relayer --interop-contract-address <INTEROP_CONTRACT_ADDRESS> --tx-mapping-contract-address <TX_MAPPING_CONTRACT_ADDRESS> --relayer-private-key <RELAYER_PRIVATE_KEY> onemoney`

This will start the relayer which will do the following steps in this order:

1. Verify if there are incomplete hash mappings for the withdrawals and complete them if possible
2. Verify if there are pending withdrawals and complete them if there are
3. Start querying checkpoints and process `BurnAndBridge` transactions found

> Note: The frequency at which the relayer queries the checkpoints can be configured using the flag `--one-money-poll-interval` which defaults to 1 second if not set.

## PoA

`relayer --interop-contract-address <INTEROP_CONTRACT_ADDRESS> --tx-mapping-contract-address <TX_MAPPING_CONTRACT_ADDRESS> --relayer-private-key <RELAYER_PRIVATE_KEY> proof-of-authority`

This will start the relayer which will do the following steps in this order:

1. Start querying for epochs and update validator sets if there was a change

> Note: The frequency at which the relayer queries the epochs can be configured using the flag `--poa-poll-interval` which defaults to 1 second if not set.

## Additional settings

### Tx Hash Mapping Recovery

When verifying that Tx Hash Mapping is complete the relayer will look for missing hashes with the 2 following mechanisms:

__Deposits__

There is an optional flag `--start-block-hash-mapping-recovery` which can be passed to specify at which block number the hash lookup should start. If the flag is not passed the relayer will start at block 0.

__Withdrawals__

There are two optional flags `--start-checkpoint-hash-mapping-recovery` and `--start-block-hash-mapping-recovery` which can be passed to specify at which checkpoint and block the hash lookup should start. If the flag is not passed the relayer will start at checkpoint 0 and at block 0.

### Transaction clearing

Upon starting, the relayer will clear pending transactions by searching for the latest completed block and latest completed checkpoint.

These values can be manually set when starting the relayer by using the flags `--from-block` and `--start-checkpoint`