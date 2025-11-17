#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 /path/to/l1client/repo"
  exit 1
fi

L1CLIENT_DIR="$(cd "$1" && pwd)"

REST_URL="http://127.0.0.1:18555"
REPLACE_VALIDATOR_ADDRESS="0x6b6c589733d3be02457257db57e5ab23a39d55f4"
FAKE_VALIDATOR_KEY="0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba"
# The fake validator (addr 0x9965...) comes from the standard anvil mnemonic
# "test test test test test test test test test test test junk" at index 5.
read -r -d '' FAKE_VALIDATOR_JSON <<'EOF' || true
{
  "consensus_public_key": "0x0337b84de6947b243626cc8b977bb1f1632610614842468dfa8f35dcbbc55a515e",
  "address": "0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc",
  "peer_id": "16Uiu2HAmGQVmprMtQwdPpJyvVZLEj3gbY3aJBmgfMzkUx3JWRydf",
  "archive": true
}
EOF

read -r -d '' RUNNING_VALIDATORS_JSON <<'EOF' || true
[
  {
    "consensus_public_key": "0x03264deb9b11fd9afb77097587466ec02c540008d97419e0b489c032183cdd4d94",
    "address": "0x3e757a1480c44eb90b5989239ef5fa24f2ebe3f9",
    "peer_id": "16Uiu2HAmFEWkpPPb1oC2yPfqtVbZGRGcR6RGN98SvRzRzfHG9fzb",
    "archive": true
  },
  {
    "consensus_public_key": "0x0246c9850cde33214a34ed324ab6ed8c24a23b05a1d5188668da4c60654b00c9a1",
    "address": "0xcf50f11805a680143f7342701085e6fb918c1a83",
    "peer_id": "16Uiu2HAkzBzk6n31JMdYBaLtTUgfPh5HCaT5HuQLStBSME623Qja",
    "archive": true
  },
  {
    "consensus_public_key": "0x036f1ca80d8af0ad900a3374875f78dbea0f60167df6b0b9583fded83eaf1aecbc",
    "address": "0x236d2b916e89696f81777fdad165cb45262ad95f",
    "peer_id": "16Uiu2HAmL8iwzfz4sbzsTveE53c4XtzhwmJpigYZWTjhvnoVb3XR",
    "archive": true
  },
  {
    "consensus_public_key": "0x0288cfa24702937bca8b55fbfdd719740c77ccb025f60c7959ff7d081622f3ac0a",
    "address": "0x6b6c589733d3be02457257db57e5ab23a39d55f4",
    "peer_id": "16Uiu2HAm4dj4PxrcicAVa6pbdvqiedtvs1mGfHVQB2tM9Ue2J5q3",
    "archive": true
  }
]
EOF
declare -A RUNNING_KEYS_BY_ADDRESS=(
  ["0x3e757a1480c44eb90b5989239ef5fa24f2ebe3f9"]="0x0b03eac696409d63e0512e2e0b39ac5be3dd58dc86b3e8b3a0ddab929c2bf693"
  ["0xcf50f11805a680143f7342701085e6fb918c1a83"]="0x87e906c8870d3bc997e529edc64344bcc69031aa31a511254c342b8c86c89bda"
  ["0x236d2b916e89696f81777fdad165cb45262ad95f"]="0xfa1c3f2b6488e9a9a83873df22b50ddbef221525bb8158fbb647f2fc6fe0a0dc"
  ["0x6b6c589733d3be02457257db57e5ab23a39d55f4"]="0x0466a99f2607d41f009e6145a661fcb91051c2c5de6d7983561b73b34a242373"
)
OPERATOR_KEY="0x76700ba1cb72480053d43b6202a16e9acbfb318b0321cfac4e55d38747bf9057"

WORK_ROOT="${WORK_ROOT:-${REPO_ROOT}/tmp/reduce-validator-set}"
FAKE_HOME="${FAKE_HOME:-${WORK_ROOT}/fake-home}"
PROFILE_WORKDIR="${PROFILE_WORKDIR:-${WORK_ROOT}/profile}"
PROFILE_NAME="${PROFILE_NAME:-default}"
mkdir -p "${WORK_ROOT}" "${FAKE_HOME}/.1m" "${PROFILE_WORKDIR}/governance"

CONFIG_FILE="${FAKE_HOME}/.1m/config.yaml"
cat >"${CONFIG_FILE}" <<EOF
global_workdir: ${PROFILE_WORKDIR}
profiles:
  ${PROFILE_NAME}:
    workdir: ${PROFILE_WORKDIR}
    network: Local
    rest_url: ${REST_URL}
EOF

CLI_BIN="${CLI_BIN:-${L1CLIENT_DIR}/target/debug/1m}"
if [[ ! -x "${CLI_BIN}" ]]; then
  echo "Building 1m CLI (first run only)..."
  (cd "${L1CLIENT_DIR}" && cargo build -p om --bin 1m >/dev/null)
fi

BEFORE_JSON="${WORK_ROOT}/epoch-before.json"
echo "Fetching current epoch data from ${REST_URL}"
curl -sfS "${REST_URL}/v1/governances/epoch" -o "${BEFORE_JSON}"
jq '.' "${BEFORE_JSON}" | sed 's/^/  /'

CURRENT_EPOCH=$(jq '.epoch_id' "${BEFORE_JSON}")
NEXT_EPOCH=$((CURRENT_EPOCH + 1))
CURRENT_VALIDATORS_JSON=$(jq '.certificate.proposal.message.validator_set.members' "${BEFORE_JSON}")
CURRENT_VALIDATOR_COUNT=$(printf '%s\n' "${CURRENT_VALIDATORS_JSON}" | jq 'length')
PREV_CERT_HASH=$(jq -r '.certificate_hash' "${BEFORE_JSON}")

PREVIOUS_CERT_PATH="${PROFILE_WORKDIR}/governance/epoch${CURRENT_EPOCH}-governance-certificate.bcs"
PREV_CERT_JSON="${WORK_ROOT}/prev-epoch.json"
curl -sfS "${REST_URL}/v1/governances/epoch/by_id?id=${CURRENT_EPOCH}&encoding=bcs" -o "${PREV_CERT_JSON}"
PREV_CERT_HEX=$(jq -r '.certificate' "${PREV_CERT_JSON}")
printf '%s' "${PREV_CERT_HEX#0x}" | tr -d '\n' | xxd -r -p > "${PREVIOUS_CERT_PATH}"

REPLACE_VALIDATOR_ADDRESS_LOWER="$(echo "${REPLACE_VALIDATOR_ADDRESS}" | tr '[:upper:]' '[:lower:]')"
RUNNING_SORTED=$(printf '%s\n' "${RUNNING_VALIDATORS_JSON}" | jq -r 'map(.address | ascii_downcase) | sort | join(",")')
TOGGLED_VALIDATORS_JSON=$(jq -n \
  --argjson running "${RUNNING_VALIDATORS_JSON}" \
  --arg replace "${REPLACE_VALIDATOR_ADDRESS_LOWER}" \
  --argjson fake "${FAKE_VALIDATOR_JSON}" '
    [ $running[] | if (.address | ascii_downcase) == $replace then $fake else . end ]
')
TOGGLED_SORTED=$(printf '%s\n' "${TOGGLED_VALIDATORS_JSON}" | jq -r 'map(.address | ascii_downcase) | sort | join(",")')
CURRENT_SORTED=$(printf '%s\n' "${CURRENT_VALIDATORS_JSON}" | jq -r 'map(.address | ascii_downcase) | sort | join(",")')

MODE=""
TARGET_VALIDATORS_JSON=""
if [[ "${CURRENT_SORTED}" == "${RUNNING_SORTED}" ]]; then
  MODE="inject_fake"
  TARGET_VALIDATORS_JSON="${TOGGLED_VALIDATORS_JSON}"
elif [[ "${CURRENT_SORTED}" == "${TOGGLED_SORTED}" ]]; then
  MODE="restore_running"
  TARGET_VALIDATORS_JSON="${RUNNING_VALIDATORS_JSON}"
else
  echo "Current validator set does not match expected running/toggled sets. Aborting."
  exit 1
fi
TARGET_VALIDATOR_COUNT=$(printf '%s\n' "${TARGET_VALIDATORS_JSON}" | jq 'length')
echo "Toggle mode: ${MODE}"
PROPOSAL_MESSAGE_JSON="${WORK_ROOT}/gov-proposal-message.json"
TIMESTAMP=$(date +%s)
jq \
  --argjson next_epoch "${NEXT_EPOCH}" \
  --argjson validators "${TARGET_VALIDATORS_JSON}" \
  --argjson ts "${TIMESTAMP}" '
  .certificate.proposal.message
  | .epoch.epoch_id = $next_epoch
  | .timestamp = $ts
  | .validator_set.members = $validators
' "${BEFORE_JSON}" > "${PROPOSAL_MESSAGE_JSON}"

REMOVED_ADDRESSES=$(jq -r --slurpfile new "${PROPOSAL_MESSAGE_JSON}" '
  (.certificate.proposal.message.validator_set.members | map(.address)) as $old
  | ($new[0].validator_set.members | map(.address)) as $next
  | [$old[] as $addr | select(($next | index($addr)) | not) | $addr]
  | if length > 0 then join(",") else "none" end
' "${BEFORE_JSON}")

PROPOSAL_MESSAGE_FILE="${WORK_ROOT}/gov-proposal-message.yaml"
yq -y '.' "${PROPOSAL_MESSAGE_JSON}" > "${PROPOSAL_MESSAGE_FILE}"
perl -0pi -e 's/fee:\n(\s+)Percentage:\n\1  /fee: !Percentage\n\1/g' "${PROPOSAL_MESSAGE_FILE}"

echo "Current epoch: ${CURRENT_EPOCH}, next epoch: ${NEXT_EPOCH}"
echo "Validator set: ${CURRENT_VALIDATOR_COUNT} -> ${TARGET_VALIDATOR_COUNT} (removed: ${REMOVED_ADDRESSES})"

PROPOSAL_BCS="${PROFILE_WORKDIR}/governance/epoch${NEXT_EPOCH}-proposal-message.bcs"
mkdir -p "$(dirname "${PROPOSAL_BCS}")"
echo "Creating governance proposal payload"
HOME="${FAKE_HOME}" "${CLI_BIN}" governance propose \
  --input-file "${PROPOSAL_MESSAGE_FILE}" \
  --output-file "${PROPOSAL_BCS}" \
  --private-key "${OPERATOR_KEY}" \
  --prev-certificate-hash "${PREV_CERT_HASH}" \
  --profile "${PROFILE_NAME}" \
  --prompt-yes

SIGNATURES_FILE="${PROFILE_WORKDIR}/governance/epoch${NEXT_EPOCH}-validator-signatures.yaml"
rm -f "${SIGNATURES_FILE}"

FAKE_ADDRESS_LOWER=$(printf '%s\n' "${FAKE_VALIDATOR_JSON}" | jq -r '.address' | tr '[:upper:]' '[:lower:]')
SIGNING_KEYS=()
mapfile -t CURRENT_ADDRESSES < <(printf '%s\n' "${CURRENT_VALIDATORS_JSON}" | jq -r '.[].address')
for addr in "${CURRENT_ADDRESSES[@]}"; do
  lower="$(echo "${addr}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${lower}" == "${FAKE_ADDRESS_LOWER}" ]]; then
    SIGNING_KEYS+=("${FAKE_VALIDATOR_KEY}")
  else
    key="${RUNNING_KEYS_BY_ADDRESS[${lower}]}"
    if [[ -z "${key:-}" ]]; then
      echo "Missing private key for validator ${addr}"
      exit 1
    fi
    SIGNING_KEYS+=("${key}")
  fi
done

idx=0
for key in "${SIGNING_KEYS[@]}"; do
  idx=$((idx + 1))
  echo "Collecting validator signature ${idx}/${#SIGNING_KEYS[@]}"
  vote_output="$(HOME="${FAKE_HOME}" "${CLI_BIN}" governance vote \
    --input-file "${PROPOSAL_BCS}" \
    --private-key "${key}" \
    --profile "${PROFILE_NAME}" 2>&1)"

  signature="$(printf '%s' "${vote_output}" | jq -r '.Result.validator_signature')"

  HOME="${FAKE_HOME}" "${CLI_BIN}" governance collect \
    "${signature}" \
    --epoch-id "${NEXT_EPOCH}" \
    --profile "${PROFILE_NAME}" >/dev/null
done

echo "Aggregating signatures from ${SIGNATURES_FILE}"
HOME="${FAKE_HOME}" "${CLI_BIN}" governance execute \
  --input-file "${PROPOSAL_BCS}" \
  --signatures-file "${SIGNATURES_FILE}" \
  --private-key "${OPERATOR_KEY}" \
  --previous-certificate "${PREVIOUS_CERT_PATH}" \
  --profile "${PROFILE_NAME}" \
  --url "${REST_URL}"

AFTER_JSON="${WORK_ROOT}/epoch-after.json"
echo "Fetching epoch data after proposal execution"
curl -sfS "${REST_URL}/v1/governances/epoch" -o "${AFTER_JSON}"
jq '.' "${AFTER_JSON}" | sed 's/^/  /'

echo "Done. Artifacts stored under ${WORK_ROOT}"
