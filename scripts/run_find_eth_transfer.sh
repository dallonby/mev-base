#!/bin/bash
# Wrapper script to run find_eth_transfer_point.py with the virtual environment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Activate virtual environment and run the script
source "$SCRIPT_DIR/venv/bin/activate"
python "$SCRIPT_DIR/find_eth_transfer_point.py" "$@"