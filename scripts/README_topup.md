# MEV Wallet Top-up Script

This script maintains MEV bot wallets at exactly 0.01 ETH balance.

## Setup

1. Add your master wallet private key to `.env`:
   ```
   MASTER_WALLET_KEY=0x...your_private_key_here...
   ```

2. Ensure you have enough ETH in the master wallet to top up all bot wallets.

## Usage

Run the script:
```bash
./scripts/topup_mev_wallets.sh
```

Or directly with Python:
```bash
python3 scripts/topup_wallets.py
```

## Features

- Checks balance of all 21 MEV bot wallets
- Calculates exact amount needed to reach 0.01 ETH
- Shows color-coded status for each wallet:
  - 🟢 Green = Exactly 0.01 ETH
  - 🟡 Yellow = Below 0.01 ETH (needs top-up)
  - 🔴 Red = Above 0.01 ETH (has excess)
- Confirms before sending transactions
- Uses minimal gas (0.005 gwei priority fee)
- Shows transaction hashes for all sent transactions

## Example Output

```
MEV Wallet Top-up Script
Master account: 0x...

Checking wallet balances...
✓ Wallet  0: 0xc0ffEe48945a9518b0B543a2C59dFb102221fBb7 - Balance: 0.010000 ETH (exact)
↑ Wallet  1: 0xc0ffee59F94F54F4F293f01672976408BC1Cad7F - Balance: 0.008500 ETH (needs 0.001500 ETH)
...

Summary:
Wallets needing top-up: 5
Total ETH needed: 0.007500 ETH

Ready to send 0.007500 ETH to 5 wallets.
Proceed? (y/N): y
```

## Notes

- The script sends transactions directly to the Base sequencer (https://mainnet.base.org)
- Transactions are sent with 21,000 gas limit (standard ETH transfer)
- A small delay (0.5s) is added between transactions to avoid nonce issues