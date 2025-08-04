#!/bin/bash
# Run the MEV results monitor continuously

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Starting MEV Results Monitor..."
echo "Press Ctrl+C to stop"

# Check if venv exists and has psycopg2
if [ -d "$SCRIPT_DIR/venv" ] && [ -f "$SCRIPT_DIR/venv/bin/python" ]; then
    # Use venv if available (has psycopg2 for DB queries)
    source "$SCRIPT_DIR/venv/bin/activate"
    python "$SCRIPT_DIR/monitor_mev_results.py" "$@"
else
    # Fall back to system python (DB queries won't work but basic monitoring will)
    python3 "$SCRIPT_DIR/monitor_mev_results.py" "$@"
fi