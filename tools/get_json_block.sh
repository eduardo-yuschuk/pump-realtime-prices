#!/usr/bin/env bash
set -euo pipefail

BLOCK_NUMBER="${1:-}"
MONIKER="${2:-mainnet}"

case "$MONIKER" in
  mainnet|mainnet-beta) RPC_URL="https://api.mainnet-beta.solana.com" ;;
  devnet)               RPC_URL="https://api.devnet.solana.com" ;;
  testnet)              RPC_URL="https://api.testnet.solana.com" ;;
  *)
    echo "Unknown cluster: ${MONIKER} (use mainnet, devnet, or testnet)" >&2
    exit 1
    ;;
esac

if [ -z "$BLOCK_NUMBER" ]; then
  BLOCK_NUMBER=$(curl -s "$RPC_URL" \
    -X POST \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"getSlot"}' \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['result'])")
  echo "No block number specified; using the latest: ${BLOCK_NUMBER}"
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OUT_DIR="${SCRIPT_DIR}/../sample_data"
mkdir -p "$OUT_DIR"
OUT_FILE="${OUT_DIR}/${BLOCK_NUMBER}.json.gz"

curl -s "$RPC_URL" \
  -X POST \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": 1,
    \"method\": \"getBlock\",
    \"params\": [
      ${BLOCK_NUMBER},
      {\"encoding\": \"json\", \"maxSupportedTransactionVersion\": 0}
    ]
  }" | python3 -m json.tool | gzip > "${OUT_FILE}"

echo "Saved: ${OUT_FILE}"
