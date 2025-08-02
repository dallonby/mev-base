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

## Database Schema

The scripts expect a `transaction_logs` table with the following structure:
- `hash`: Transaction hash
- `source`: Where the transaction was seen (e.g., flashblock_0)
- `timestamp`: When the transaction was first logged
- `block_number`: Block number of the transaction
- `sources`: Array of all sources that have seen this transaction