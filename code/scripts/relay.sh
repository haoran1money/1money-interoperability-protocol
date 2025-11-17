#!/usr/bin/env bash
IFS=$'\n\t'

# Resolve important paths
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly ROOT_DIR
ONEMONEY_DIR="$(cd "${ROOT_DIR}/../1money-l1client" && pwd)"
readonly ONEMONEY_DIR
INTEROP_DIR="$(cd "${ROOT_DIR}/../1money-interoperability-protocol" && pwd)"
readonly INTEROP_DIR
MNEMONIC="test test test test test test test test test test test junk"
readonly MNEMONIC

cd "${ROOT_DIR}"

# Step 0: Build client binaries (1m + relayer)
cd "${ONEMONEY_DIR}"
cargo build --bin 1m
cd "${ROOT_DIR}"/code
cargo build -p relayer
PATH="${ONEMONEY_DIR}/target/debug:${PATH}"
cd "${INTEROP_DIR}"/code
cargo build -p relayer
PATH="${INTEROP_DIR}/code/target/debug:${PATH}"

cd "${ROOT_DIR}"

# Step 1: Configure runtime parameters
export ONEMONEY_RPC="${ONEMONEY_RPC:-http://127.0.0.1:18555}"
export SIDECHAIN_RPC="${SIDECHAIN_RPC:-http://127.0.0.1:8645}"
export SIDECHAIN_WS="${SIDECHAIN_WS:-http://127.0.0.1:8646}"
export PROFILE="${PROFILE:-interop-demo}"
export TOKEN_SYMBOL="${TOKEN_SYMBOL:-OMTST$(date +%s)}"
export TOKEN_NAME="${TOKEN_NAME:-Interop_Demo_Token}"
export TOKEN_DECIMALS="${TOKEN_DECIMALS:-6}"
export BRIDGE_AMOUNT="${BRIDGE_AMOUNT:-750}"
export ESCROW_FEE="${ESCROW_FEE:-5}"
export ACCOUNTS_DIR="${ACCOUNTS_DIR:-${ROOT_DIR}/code/scripts/.manual/accounts}"
export OPERATOR_PRIVATE_KEY="${OPERATOR_PRIVATE_KEY:-0x76700ba1cb72480053d43b6202a16e9acbfb318b0321cfac4e55d38747bf9057}"

# Step 2: Derive all needed keys and addresses
RELAYER_PRIVATE_KEY="$(cast wallet --derive-private-key "${MNEMONIC}" --mnemonic-index 0)" # sidechain's validator manager contract owner
export RELAYER_PRIVATE_KEY
OWNER_PRIVATE_KEY="$(cast wallet --derive-private-key "${MNEMONIC}" --mnemonic-index 1)"
SC_TOKEN_PRIVATE_KEY="$(cast wallet --derive-private-key "${MNEMONIC}" --mnemonic-index 2)"
USER_PRIVATE_KEY="$(cast wallet --derive-private-key "${MNEMONIC}" --mnemonic-index 3)"

RELAYER_ADDRESS="$(cast wallet address --private-key "${RELAYER_PRIVATE_KEY}")"
OWNER_ADDRESS="$(cast wallet address --private-key "${OWNER_PRIVATE_KEY}")"
SC_TOKEN_ADDRESS="$(cast wallet address --private-key "${SC_TOKEN_PRIVATE_KEY}")"
USER_ADDRESS="$(cast wallet address --private-key "${USER_PRIVATE_KEY}")"

OPERATOR_ADDRESS="$(cast wallet address --private-key "${OPERATOR_PRIVATE_KEY}")"
FUND_AMOUNT="${FUND_AMOUNT:-5ether}"
cast send "${OPERATOR_ADDRESS}" --value "${FUND_AMOUNT}" --rpc-url "${SIDECHAIN_RPC}" --private-key "${OWNER_PRIVATE_KEY}"

# Step 3: Import accounts into the OneMoney profile
mkdir -p "${ACCOUNTS_DIR}"
1m account import --secret-key-hex "${OPERATOR_PRIVATE_KEY}" --name operator --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"
1m account import --secret-key-hex "${RELAYER_PRIVATE_KEY}" --name relayer --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"
1m account import --secret-key-hex "${USER_PRIVATE_KEY}" --name user --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"

# Step 4: Build on-chain contracts and deploy OMInterop
cd code
forge build
cd "${ROOT_DIR}"
OMINTEROP_BYTECODE="$(jq -r '.bytecode.object' code/solidity/out/OMInterop.sol/OMInterop.json)"
DEPLOY_OUTPUT="$(cast send --json --rpc-url "${SIDECHAIN_RPC}" --private-key "${OWNER_PRIVATE_KEY}" --create "${OMINTEROP_BYTECODE}" "constructor(address,address,address)" "${OWNER_ADDRESS}" "${OPERATOR_ADDRESS}" "${RELAYER_ADDRESS}")"
INTEROP_CONTRACT_ADDRESS="$(echo "${DEPLOY_OUTPUT}" | jq -r '.contractAddress')"
export INTEROP_CONTRACT_ADDRESS

echo "RELAYER_PRIVATE_KEY=${RELAYER_PRIVATE_KEY}"
echo "INTEROP_CONTRACT_ADDRESS=${INTEROP_CONTRACT_ADDRESS}"

# Step 5: Issue, grant permissions, and map the token
TOKEN_ISSUE_OUTPUT="$(1m account issue "${TOKEN_SYMBOL}" "${TOKEN_NAME}" "${TOKEN_DECIMALS}" --master-authority "${OPERATOR_ADDRESS}" --signer operator --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}" 2>&1)"
echo "${TOKEN_ISSUE_OUTPUT}"

OM_TOKEN="$(echo "${TOKEN_ISSUE_OUTPUT}" | jq -r '.Result.response.token_address')"
1m token get "${OM_TOKEN}" --profile "${PROFILE}" --url "${ONEMONEY_RPC}"

cast send "${INTEROP_CONTRACT_ADDRESS}" "mapTokenAddresses(address,address,uint8)" "${OM_TOKEN}" "${SC_TOKEN_ADDRESS}" 1 --rpc-url "${SIDECHAIN_RPC}" --private-key "${OPERATOR_PRIVATE_KEY}"

1m account grant "${OM_TOKEN}" Bridge "${RELAYER_ADDRESS}" --value 1000000000000000000000000000000 --signer operator --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"


# Step 6: run the relayer in the background
echo "export RELAYER_PRIVATE_KEY=${RELAYER_PRIVATE_KEY}"
echo "export INTEROP_CONTRACT_ADDRESS=${INTEROP_CONTRACT_ADDRESS}"

cd code
cargo run --bin relayer -- all
cd "${ROOT_DIR}"

# Step 7: Run the bridge happy-path flow
1m account balance "${OM_TOKEN}" "${USER_ADDRESS}" --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"

cast send "${INTEROP_CONTRACT_ADDRESS}" "bridgeFrom(address,uint256)" "${USER_ADDRESS}" "755000000" --rpc-url "${SIDECHAIN_RPC}" --private-key "${SC_TOKEN_PRIVATE_KEY}"

1m account balance "${OM_TOKEN}" "${USER_ADDRESS}" --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"

# Step 8: Burn, reconcile, and report balances
1m account transfer "${OM_TOKEN}" "${RELAYER_ADDRESS}" "${ESCROW_FEE}" --signer user --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}" # temporary; fee collection logic not yet implemented in relayer

1m account burn-bridge "${OM_TOKEN}" "${USER_ADDRESS}" "${BRIDGE_AMOUNT}" --destination-chain-id 1 --destination-address "${SC_TOKEN_ADDRESS}" --escrow-fee "${ESCROW_FEE}" --signer user --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"

1m account balance "${OM_TOKEN}" "${USER_ADDRESS}" --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"
1m account balance "${OM_TOKEN}" "${RELAYER_ADDRESS}" --profile "${PROFILE}" --url "${ONEMONEY_RPC}" --workdir "${ACCOUNTS_DIR}"

# Step 9: PoA validator set updates.

cast call 0x0000000000000000000000000000000000002000 "getValidatorAddresses()(address[])" --rpc-url "${SIDECHAIN_RPC}"
curl -s "${ONEMONEY_RPC}/v1/governances/epoch" | jq -r ".certificate.proposal.message.validator_set.members[].address"

bash "${ROOT_DIR}/code/scripts/toggle_validator_set.sh" "${ONEMONEY_DIR}"
