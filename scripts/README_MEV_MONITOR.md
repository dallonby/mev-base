# MEV Results Monitor

## Overview

The MEV Results Monitor tracks the outcome of MEV transactions submitted by our bot and provides detailed analysis when transactions fail to be included on-chain.

## How It Works

1. **Monitors `mev_results.jsonl`**: Watches for new MEV opportunities that were submitted
2. **Checks Transaction Status**: Uses `cast` to check if our transaction made it on-chain
3. **Analyzes Failures**: When a transaction isn't included, runs `find_eth_transfer_point.py` to identify who beat us

## Usage

### Continuous Monitoring
```bash
# Monitor new entries as they arrive
./scripts/monitor_mev.sh

# Check every 10 seconds instead of default 5
./scripts/monitor_mev.sh --interval 10
```

### One-Time Analysis
```bash
# Analyze the last 10 MEV results
python3 scripts/monitor_mev_results.py --last 10

# Check all entries in the file once
python3 scripts/monitor_mev_results.py --once
```

## Output Interpretation

### Success ✅
```
Status: SUCCESS
✅ Transaction succeeded!
   Gas Used: 391,664
   Gas Price: 0.1111 gwei
```
Your transaction was included and executed successfully.

### Reverted ❌
```
Status: REVERTED
❌ Transaction reverted on-chain
   Reason: Transaction reverted on-chain
```
Your transaction was included but failed during execution (someone beat you).

### Not Included ❌
```
Status: NOT_INCLUDED
❌ Transaction not included in block

🏁 BEATEN BY:
   TX Hash: 0x123...
   Sender: 0xabc...
   Gas Price: 0.2345 gwei
   TX Index: 42
   Same Flashblock: YES
   ⚡ Lost in same flashblock! Need higher gas price.
```

When your transaction isn't included, the monitor shows:
- **Who beat you**: Transaction hash and sender address
- **Gas price comparison**: Their gas price vs yours
- **Flashblock analysis**:
  - Same flashblock = You need higher gas price
  - Different flashblock = You need better timing/positioning

## Key Insights

### Same Flashblock Loss
If you lose within the same flashblock, it's purely a gas price issue. Consider:
- Increasing the profit percentage allocated to gas (currently 15%)
- Reducing the gas price cap (currently 1 gwei)
- Analyzing competitor gas pricing patterns

### Different Flashblock Loss
If you lose to an earlier flashblock, consider:
- Your transaction arrived too late
- Network propagation delays
- Sequencer ordering preferences

## Integration with Main System

The monitor runs independently but could be integrated to:
1. Automatically adjust gas pricing based on loss patterns
2. Track competitor addresses and their strategies
3. Build a database of winning gas prices by opportunity type
4. Alert on repeated losses to same competitors

## Files

- `scripts/monitor_mev_results.py` - Main monitoring script
- `scripts/monitor_mev.sh` - Convenience wrapper
- `scripts/find_eth_transfer_point.py` - Analyzes who won the MEV opportunity
- `mev_results.jsonl` - Log of all MEV submissions with expected profits