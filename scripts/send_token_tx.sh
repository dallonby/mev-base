#!/bin/bash

# Check if we have exactly 3 arguments
if [ $# -ne 3 ]; then
    echo "Usage: $0 <token_type> <contract_address> <token_id>"
    echo "  token_type: 'erc721' or 'erc1155'"
    echo "  contract_address: Contract address (0x...)"
    echo "  token_id: Token ID (decimal number)"
    echo ""
    echo "Example: $0 erc721 0x23a5e200a37bad403d1b3181f5cec072e381cae6 12456"
    echo ""
    echo "Note: Requires PRIVATE_KEY environment variable to be set"
    exit 1
fi

# Check if PRIVATE_KEY is set
if [ -z "$PRIVATE_KEY" ]; then
    echo "Error: PRIVATE_KEY environment variable is not set"
    echo "Please set it with: export PRIVATE_KEY=your_private_key"
    exit 1
fi

TOKEN_TYPE=$1
CONTRACT_ADDRESS=$2
TOKEN_ID=$3

# Target address
TARGET="0x5a083a0000d3f2817872b6006e000000007574ca"

# Validate token type
if [ "$TOKEN_TYPE" != "erc721" ] && [ "$TOKEN_TYPE" != "erc1155" ]; then
    echo "Error: token_type must be 'erc721' or 'erc1155'"
    exit 1
fi

# Validate contract address format
if [[ ! "$CONTRACT_ADDRESS" =~ ^0x[a-fA-F0-9]{40}$ ]]; then
    echo "Error: Invalid contract address format. Must be 0x followed by 40 hex characters"
    exit 1
fi

# Remove 0x prefix from contract address and pad to 64 chars
CONTRACT_HEX=$(echo $CONTRACT_ADDRESS | sed 's/0x//')
CONTRACT_PADDED=$(printf "%064s" $CONTRACT_HEX | tr ' ' '0')

# Convert token ID to hex and pad to 64 chars
TOKEN_ID_HEX=$(printf "%064x" $TOKEN_ID)

# Construct calldata based on token type
if [ "$TOKEN_TYPE" == "erc721" ]; then
    # Function selector: 0xf3e414f8
    CALLDATA="0xf3e414f8${CONTRACT_PADDED}${TOKEN_ID_HEX}"
elif [ "$TOKEN_TYPE" == "erc1155" ]; then
    # Function selector: 0xa1538bde
    # Additional fixed parameters for ERC1155
    CALLDATA="0xa1538bde${CONTRACT_PADDED}${TOKEN_ID_HEX}000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000"
fi

# Display transaction details
echo "Sending transaction:"
echo "  To: $TARGET"
echo "  Value: 0"
echo "  Token Type: $TOKEN_TYPE"
echo "  Contract: $CONTRACT_ADDRESS"
echo "  Token ID: $TOKEN_ID (0x${TOKEN_ID_HEX})"
echo "  Calldata: $CALLDATA"
echo ""

# Send transaction using cast with PRIVATE_KEY
echo "Executing: cast send $TARGET \"$CALLDATA\" --value 0 --rpc-url https://mainnet.base.org --private-key \$PRIVATE_KEY"
cast send "$TARGET" "$CALLDATA" --value 0 --rpc-url https://mainnet.base.org --private-key $PRIVATE_KEY