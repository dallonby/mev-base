#!/bin/bash
# Top up MEV bot wallets to 0.01 ETH

cd "$(dirname "$0")/.."
python3 scripts/topup_wallets.py "$@"