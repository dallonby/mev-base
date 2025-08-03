# MEV Analysis Scripts

This directory contains Python scripts for analyzing MEV opportunities and transaction data.

## Setup

1. Create and activate a Python virtual environment:
   ```bash
   cd scripts
   python3 -m venv venv
   source venv/bin/activate
   ```

2. Install dependencies:
   ```bash
   pip install -r requirements.txt
   ```

3. Make sure PostgreSQL credentials are set in your `.env` file:
   ```
   POSTGRES_HOST=localhost
   POSTGRES_PORT=5432
   POSTGRES_DB=backrunner_db
   POSTGRES_USER=backrunner
   POSTGRES_PASSWORD=your_password_here
   ```

## Scripts

### find_eth_transfer_point.py

Analyzes transactions to find where ETH transfers exceed msg.value by scanning backwards through block indices.

**Features:**
- Uses debug_traceCallMany to simulate calls at different block states
- Queries PostgreSQL to show when transactions were first seen
- Calculates time difference between original tx and MEV opportunity
- Reports effective gas price for transactions

**Usage:**
```bash
# Using the wrapper script (recommended)
./run_find_eth_transfer.sh <transaction_hash>

# Or directly with venv
source venv/bin/activate
python find_eth_transfer_point.py <transaction_hash>

# Test specific trace
python find_eth_transfer_point.py --trace-specific
```

### test_db_query.py

Tests PostgreSQL connectivity and query functionality.

**Usage:**
```bash
source venv/bin/activate
python test_db_query.py
```

### scan_events.py

Scans the Base blockchain for events with a specific topic0 signature.

**Features:**
- Takes any topic0 as command line argument
- Configurable block range and chunk size
- Groups results by contract address
- Shows event statistics and sample data
- Optional JSON output

**Usage:**
```bash
# Scan for Transfer events (last 24 hours)
python scan_events.py 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef

# Scan last 10,000 blocks for a specific event
python scan_events.py 0x5380355699fac5266e4d95cf6985cf6a48abe03aa33d07723bdd0338a367af25 -b 10000

# Save results to file
python scan_events.py 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef -o transfers.json

# Use custom chunk size for faster scanning
python scan_events.py 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef -c 10000
```

## Database Schema

The scripts expect a `transaction_logs` table with the following structure:
- `hash`: Transaction hash
- `source`: Where the transaction was seen (e.g., flashblock_0)
- `timestamp`: When the transaction was first logged
- `block_number`: Block number of the transaction
- `sources`: Array of all sources that have seen this transaction