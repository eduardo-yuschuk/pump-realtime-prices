#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPOSITORY_ROOT=$(cd "${SCRIPT_DIR}/.." && pwd)
INSPECTOR="${REPOSITORY_ROOT}/target/debug/visual-inspector"
CATALOG="${SCRIPT_DIR}/pumpfun.md"

SIGNATURES=(
  "4MqjMKirGSkrCFb28igxVogFPusuuVHjg5YFoAsfToRPw2PvkvQLSBpgGu54TkMtoHFCVCQ41aRT69gHJ7ZWvgBu"
  "5GrtNwGPxpSCJGetScs4LKRNzeA6HCPH8N4SXB65Znzem1jzEQP4FtWDvckvXoEaJwvJe7oYGvWoSmFkGm5ry8Kh"
  "42wvgXDw9k7q2gL2TMDX4DfGr8qcsTqC3YWZApHyQ8aS3jGKBRTHDACh1HckXFXdpD7SSFgmjeWVCL73jPiHuL5r"
  "ack8cfBKjGrymVL2ch9eaP7VCFDqt2s3LGduSUZyGj8wTS3P1LFeH1Q8ZH9SZdTtmGSudKhLX3jRSRX7RCrM8TL"
  "5Ym6NbFb7FRUo1p2884Ha6qitFrcUDVJA4h5aiYet4RRoXysajypjdgSnBtJhC5ug1Ldx2hhcCL5x1yjosAqzBRh"
  "35ehzs6PkA2W68uGVWB2xRFsAQ8mx1TUytr8n8UKGaprESz4sZxGzzLFpnwPm4a1HiCpsBVMd4vnmJTP7DFmp6x7"
  "2yBYoy1SGNZPbrKtbWKrj2EHjVzp5fCpUHc6AN2G7RBMGyUA9gBoxGids7JSp5zZDgqkAb3TwSA7Rf4VJBmvBunk"
)

print_reference() {
  local signature="$1"
  local reference

  if ! reference=$(awk -v signature="${signature}" '
    $0 == "## `" signature "`" { found = 1; next }
    found && /^## `/ { exit }
    found && NF { print }
    END { if (!found) exit 1 }
  ' "${CATALOG}"); then
    printf 'Missing reference data for transaction %s\n' "${signature}" >&2
    return 1
  fi

  printf '%s\n' "${reference}"
}

NO_DNA=1 cargo build \
  --quiet \
  --manifest-path "${REPOSITORY_ROOT}/Cargo.toml" \
  --package visual_inspector \
  --bin visual-inspector

for signature in "${SIGNATURES[@]}"; do
  printf '\n=== %s ===\n' "${signature}"
  printf '\nReference data:\n'
  print_reference "${signature}"
  printf '\nParsed result:\n'
  "${INSPECTOR}" transaction "${signature}" pumpfun
done
