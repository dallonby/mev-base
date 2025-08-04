#!/bin/bash

# Test the binary search optimizer with debug logging

echo "Testing binary search optimizer with debug logging..."

# Set environment variables for debugging
export MEV_LOG=debug
export RUST_LOG=debug

# Run the MEV service
cd /home/ubuntu/Source/reth-mev-standalone
cargo run -p mevbase --profile=release -- \
    --node.rpc-http-url http://localhost:8545 \
    --node.rpc-ws-url ws://localhost:8546 \
    --sequencer.endpoint http://localhost:8123/bundle \
    --sequencer.auth-header "X-Flashbots-Signature: test" \
    --database.url postgres://mev:mev@localhost/mev_db