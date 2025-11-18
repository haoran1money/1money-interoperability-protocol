# Onemoney Interoperability Protocol - Devnet Setup Guide

Run every command from the `1money-interoperability-protocol` repo root.

## Prerequisites

- l1client tools are built and available on `PATH`.
  - Example: `export PATH=$PATH:$PATH_TO_L1CLIENT_DIR/target/debug`
- 1money devnet, running 1money endpoint at `http://127.0.0.1:18555` by
  `./testnet.sh -d`.
  - Pass `--min-tx-to-checkpoint 1` in
    [`./testnet.sh`](https://github.com/1Money-Co/l1client/blob/c407fcf6571468935f4cc041998ceac16a00daea/testnet.sh#L41)
    to generate checkpoints quickly for testing.
  - Default validator set of size 4.
    - Validator addresses:
      `curl -s http://127.0.0.1:18555/v1/governances/epoch | jq -r ".certificate.proposal.message.validator_set.members[].address"`
    - 1money validator private keys are present at
      `/tmp/1m-network/node*/conf/consensus_secret_key.hex`.
  - With the default operator private key:
    `0x76700ba1cb72480053d43b6202a16e9acbfb318b0321cfac4e55d38747bf9057`.
- A sidechain Emerald node, running _eth_ endpoint at `http://127.0.0.1:8645`
  and _ws_ endpoint at `ws://127.0.0.1:8646`.
  - With PoA validator set matching Onemoney's validator set.
  - Refer to [sidechain readme](sidechain.md).
- `cargo`,`jq`, and `foundry` tools (`cast` and `forge`) are installed and
  available on `PATH`.

## Configure runtime parameters

Update the values below before running the block so they match your local RPCs,
accounts directory, desired token metadata, and deployer key.

```bash
MNEMONIC="test test test test test test test test test test test junk"
ONEMONEY_RPC=http://127.0.0.1:18555
SIDECHAIN_RPC=http://127.0.0.1:8645
SIDECHAIN_WS=http://127.0.0.1:8646
PROFILE=interop-demo
TOKEN_SYMBOL=OMTST$(date +%s)
TOKEN_NAME=Interop_Demo_Token
TOKEN_DECIMALS=18 # use same decimals as sidechain's ERC20 token
BRIDGE_AMOUNT=750
ESCROW_FEE=5
OM_ACCOUNTS_DIR=$(pwd)/code/scripts/.manual/accounts
OPERATOR_PRIVATE_KEY=0x76700ba1cb72480053d43b6202a16e9acbfb318b0321cfac4e55d38747bf9057
OPERATOR_FAUCET_AMOUNT=5ether
```

## Setup accounts and deploy OMInterop contract

### Derive keys and fund the Sidechain

```bash
RELAYER_PRIVATE_KEY=$(cast wallet --derive-private-key "$MNEMONIC" --mnemonic-index 0)
OWNER_PRIVATE_KEY=$(cast wallet --derive-private-key "$MNEMONIC" --mnemonic-index 1)
SC_TOKEN_PRIVATE_KEY=$(cast wallet --derive-private-key "$MNEMONIC" --mnemonic-index 2)
USER_PRIVATE_KEY=$(cast wallet --derive-private-key "$MNEMONIC" --mnemonic-index 3)

RELAYER_ADDRESS=$(cast wallet address --private-key $RELAYER_PRIVATE_KEY)
OWNER_ADDRESS=$(cast wallet address --private-key $OWNER_PRIVATE_KEY)
SC_TOKEN_ADDRESS=$(cast wallet address --private-key $SC_TOKEN_PRIVATE_KEY)
USER_ADDRESS=$(cast wallet address --private-key $USER_PRIVATE_KEY)
OPERATOR_ADDRESS=$(cast wallet address --private-key $OPERATOR_PRIVATE_KEY)

cast send $OPERATOR_ADDRESS --value $OPERATOR_FAUCET_AMOUNT --rpc-url $SIDECHAIN_RPC --private-key $OWNER_PRIVATE_KEY
```

### Import accounts into Onemoney

```bash
mkdir -p $OM_ACCOUNTS_DIR
1m account import --secret-key-hex $OPERATOR_PRIVATE_KEY --name operator --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
1m account import --secret-key-hex $RELAYER_PRIVATE_KEY --name relayer --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
1m account import --secret-key-hex $USER_PRIVATE_KEY --name user --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
```

### Build Solidity and deploy OMInterop

```bash
cd code
forge build
cd ..
OMINTEROP_BYTECODE=$(jq -r '.bytecode.object' code/solidity/out/OMInterop.sol/OMInterop.json)
# deploy OMInterop contract
DEPLOY_OUTPUT=$(cast send --json --rpc-url $SIDECHAIN_RPC --private-key $OWNER_PRIVATE_KEY --create $OMINTEROP_BYTECODE "constructor(address,address,address)" $OWNER_ADDRESS $OPERATOR_ADDRESS $RELAYER_ADDRESS)
# extract deployed contract address
INTEROP_CONTRACT_ADDRESS=$(echo $DEPLOY_OUTPUT | jq -r '.contractAddress')

# print information for next steps
echo RELAYER_PRIVATE_KEY=$RELAYER_PRIVATE_KEY
echo INTEROP_CONTRACT_ADDRESS=$INTEROP_CONTRACT_ADDRESS
```

## Issue token at Onemoney and map it with Sidechain token

```bash
# issue token on Onemoney
TOKEN_ISSUE_OUTPUT=$(1m account issue $TOKEN_SYMBOL $TOKEN_NAME $TOKEN_DECIMALS --master-authority $OPERATOR_ADDRESS --signer operator --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR 2>&1)
echo $TOKEN_ISSUE_OUTPUT
# extract issued token address
OM_TOKEN=$(echo $TOKEN_ISSUE_OUTPUT | jq -r '.Result.response.token_address')
# print information for next steps
echo OM_TOKEN=$OM_TOKEN
# verify issued token
1m token get $OM_TOKEN --profile $PROFILE --url $ONEMONEY_RPC

# map token addresses in the interop contract
cast send $INTEROP_CONTRACT_ADDRESS "mapTokenAddresses(address,address,uint8)" $OM_TOKEN $SC_TOKEN_ADDRESS 1 --rpc-url $SIDECHAIN_RPC --private-key $OPERATOR_PRIVATE_KEY
```

## Start the Relayer

### Grant Bridge permissions to relayer on Onemoney

```bash
# grant Bridge permission to relayer
1m account grant $OM_TOKEN Bridge $RELAYER_ADDRESS --value 1000000000000000000000000000000 --signer operator --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
```

### Start the relayer in different terminal

```bash
cd code
cargo run --bin relayer -- --relayer-private-key $RELAYER_PRIVATE_KEY --interop-contract-address $INTEROP_CONTRACT_ADDRESS all
# keep running... and observe bridge logs
```

> [!NOTE]
> Use `--help` flag to see all available relayer options.

## Bridge operations

### Deposit from sidechain to Onemoney

```bash
# pre-deposit user balance
1m account balance $OM_TOKEN $USER_ADDRESS --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
# perform deposit from sidechain; ideally this is called by LayerZero OFT contract
cast send $INTEROP_CONTRACT_ADDRESS "bridgeFrom(address,uint256)" $USER_ADDRESS 755000000 --rpc-url $SIDECHAIN_RPC --private-key $SC_TOKEN_PRIVATE_KEY
# check the user balance after deposit; it should be 755
```

### Withdrawal from Onemoney to Sidechain

```bash
# temporary transfer bridge fee to relayer account; won't need when burn-bridge does this automatically
1m account transfer $OM_TOKEN $RELAYER_ADDRESS $ESCROW_FEE --signer user --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
# perform withdrawal from Onemoney
1m account burn-bridge $OM_TOKEN $USER_ADDRESS $BRIDGE_AMOUNT --destination-chain-id 1 --destination-address $SC_TOKEN_ADDRESS --escrow-fee $ESCROW_FEE --signer user --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
# verify post-withdrawal balances
1m account balance $OM_TOKEN $USER_ADDRESS --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
1m account balance $OM_TOKEN $RELAYER_ADDRESS --profile $PROFILE --url $ONEMONEY_RPC --workdir $OM_ACCOUNTS_DIR
```

### PoA validator set update from Onemoney to Sidechain

Replace `$PATH_TO_L1CLIENT_DIR` with the path to the local `l1client` repo
directory.

```bash
# view current validator set on Onemoney
curl -s $ONEMONEY_RPC/v1/governances/epoch | jq -r ".certificate.proposal.message.validator_set.members[].address"
# view current validator set on sidechain
cast call 0x0000000000000000000000000000000000002000 "getValidatorAddresses()(address[])" --rpc-url $SIDECHAIN_RPC
# toggle validator set on Onemoney (this also triggers sidechain update via relayer)
bash code/scripts/toggle_validator_set.sh $PATH_TO_L1CLIENT_DIR
# view the updated validator set on sidechain
```
