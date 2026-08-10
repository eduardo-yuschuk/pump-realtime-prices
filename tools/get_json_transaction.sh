#!/usr/bin/env bash
set -euo pipefail

SIGNATURE="${1:-}"
MONIKER="${2:-mainnet}"

if [ "$#" -gt 2 ] || [ -z "$SIGNATURE" ]; then
  echo "Usage: $0 <transaction-signature> [mainnet|devnet|testnet]" >&2
  exit 1
fi

if [[ ! "$SIGNATURE" =~ ^[1-9A-HJ-NP-Za-km-z]{64,88}$ ]]; then
  echo "Invalid transaction signature: expected 64 to 88 base58 characters" >&2
  exit 1
fi

case "$MONIKER" in
  mainnet|mainnet-beta) RPC_URL="https://api.mainnet-beta.solana.com" ;;
  devnet)               RPC_URL="https://api.devnet.solana.com" ;;
  testnet)              RPC_URL="https://api.testnet.solana.com" ;;
  *)
    echo "Unknown cluster: ${MONIKER} (use mainnet, devnet, or testnet)" >&2
    exit 1
    ;;
esac

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OUT_DIR="${SCRIPT_DIR}/../sample_data"
mkdir -p "$OUT_DIR"
OUT_FILE="${OUT_DIR}/${SIGNATURE}.json.gz"
TEMP_FILE=$(mktemp "${OUT_FILE}.tmp.XXXXXX")
trap 'rm -f "$TEMP_FILE"' EXIT

curl --fail-with-body --silent --show-error "$RPC_URL" \
  -X POST \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": 1,
    \"method\": \"getTransaction\",
    \"params\": [
      \"${SIGNATURE}\",
      {
        \"commitment\": \"finalized\",
        \"encoding\": \"json\",
        \"maxSupportedTransactionVersion\": 0
      }
    ]
  }" \
  | python3 -c '
import json
import sys

signature = sys.argv[1]
response = json.load(sys.stdin)

if "error" in response:
    error = response["error"]
    code = error.get("code", "unknown") if isinstance(error, dict) else "unknown"
    message = error.get("message", error) if isinstance(error, dict) else error
    raise SystemExit(f"Solana RPC error {code}: {message}")

result = response.get("result")
if result is None:
    raise SystemExit(f"Transaction not found or not finalized: {signature}")

try:
    returned_signature = result["transaction"]["signatures"][0]
except (KeyError, IndexError, TypeError):
    raise SystemExit("Solana RPC response does not contain a transaction signature")

if returned_signature != signature:
    raise SystemExit(
        f"Solana RPC returned signature {returned_signature}, expected {signature}"
    )

json.dump(response, sys.stdout, indent=4)
sys.stdout.write("\n")
' "$SIGNATURE" \
  | gzip > "$TEMP_FILE"

mv "$TEMP_FILE" "$OUT_FILE"
trap - EXIT

echo "Saved: ${OUT_FILE}"
