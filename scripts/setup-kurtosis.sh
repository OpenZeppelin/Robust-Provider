#!/usr/bin/env bash

#
# WARNING: This script was AI generated 
#
# Setup Kurtosis ethereum-package devnet and output EL endpoints as JSON.
#
# Usage:
#   ./scripts/setup-kurtosis.sh 
#
# Output: target/kurtosis-endpoints.json
#
# JSON format:
# [
#   { "client": "geth", "http": "http://127.0.0.1:32003", "ws": "ws://127.0.0.1:32004" },
#   ...
# ]

set -euo pipefail

ENCLAVE="local-eth-testnet"
OUTPUT_DIR="target"
OUTPUT_FILE="$OUTPUT_DIR/kurtosis-endpoints.json"

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR"

# Check if kurtosis is installed
if ! command -v kurtosis &> /dev/null; then
    echo "Error: kurtosis CLI not found. Please install it first." >&2
    echo "See: https://docs.kurtosis.com/install" >&2
    exit 1
fi

# Check if kurtosis engine is running
if ! kurtosis engine status &> /dev/null; then
    echo "Starting Kurtosis engine..."
    kurtosis engine start
fi

# Check if enclave exists
enclave_exists() {
    kurtosis enclave ls 2>/dev/null | grep -q "^[^ ]*[[:space:]]*$ENCLAVE[[:space:]]"
}

if ! enclave_exists; then
    echo "Enclave '$ENCLAVE' not found. Creating new enclave with ethereum-package..."
    kurtosis run --enclave "$ENCLAVE" github.com/ethpandaops/ethereum-package --args-file ./tests/common/network_params.yaml
fi

echo "Inspecting enclave '$ENCLAVE'..."

# Get list of EL services (lines starting with "el-")
el_services=$(kurtosis enclave inspect "$ENCLAVE" 2>/dev/null | grep -E "^[a-f0-9]+[[:space:]]+el-" | awk '{print $2}')

if [ -z "$el_services" ]; then
    echo "Error: No EL services found in enclave '$ENCLAVE'" >&2
    exit 1
fi

# Build JSON array
json="["
first=true

for service in $el_services; do
    # Extract client name from service name (e.g., "el-1-geth-lighthouse" -> "geth")
    # Format: el-{index}-{client}-{cl_client}
    client=$(echo "$service" | sed -E 's/^el-[0-9]+-([^-]+)-.*/\1/')
    
    # Get port mappings
    rpc_addr=$(kurtosis port print "$ENCLAVE" "$service" rpc 2>/dev/null || echo "")
    ws_addr=$(kurtosis port print "$ENCLAVE" "$service" ws 2>/dev/null || echo "")
    
    if [ -z "$rpc_addr" ]; then
        echo "Warning: Could not get RPC port for $service, skipping..." >&2
        continue
    fi
    
    # Build JSON object
    if [ "$first" = true ]; then
        first=false
    else
        json+=","
    fi
    
    http_url="http://$rpc_addr"
    
    if [ -n "$ws_addr" ]; then
        ws_url="ws://$ws_addr"
        json+=$(printf '\n  {"client": "%s", "http": "%s", "ws": "%s"}' "$client" "$http_url" "$ws_url")
    else
        json+=$(printf '\n  {"client": "%s", "http": "%s", "ws": null}' "$client" "$http_url")
    fi
done

json+="\n]"

# Write JSON to file
printf "$json\n" > "$OUTPUT_FILE"

echo "Wrote endpoints to $OUTPUT_FILE:"
cat "$OUTPUT_FILE"
