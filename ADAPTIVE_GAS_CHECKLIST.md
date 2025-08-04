# Adaptive Gas Management Deployment Checklist

## Before Running

1. **Check Redis is running with auth:**
   ```bash
   redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq ping
   ```

2. **Build with release profile:**
   ```bash
   cargo build -p mevbase --profile=release
   ```

## What to Monitor

### 1. Initial Gas Consumption
- First run on any contract will use full bounds (no history)
- Watch logs for "Gas used" - should be 30-40M ideally
- If you see 60M+ gas, the system will narrow bounds automatically

### 2. Gas History Building
- Check Redis for stored values:
  ```bash
  redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq --scan --pattern "mev:gas:*"
  ```
- Values update after each optimization with IIR filtering (α=0.05)

### 3. Log Patterns to Watch

**Good signs:**
```
Stored gas history in Redis
Retrieved gas history from Redis  
Adjusting bounds based on gas history
```

**Warning signs:**
```
Failed to connect to Redis for gas history
Failed to store gas history in Redis
Gas used: 100000000+ (very high)
```

### 4. Performance Improvements
- V4 should no longer halt with OutOfGas
- Profitable opportunities should start appearing again
- Gas consumption should stabilize around 30-40M per scan

## Quick Diagnostics

**Check current gas history:**
```bash
# See all stored gas values
redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq --scan --pattern "mev:gas:*" | \
while read key; do 
  echo -n "$key = "
  redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq GET "$key"
done
```

**Monitor MEV results:**
```bash
tail -f mev_results.jsonl | jq '.'
```

**Check for high gas contracts:**
```bash
grep "Gas used" logs | awk '$3 > 50000000 {print}'
```

## Troubleshooting

1. **No gas history stored:**
   - Check Redis auth is correct
   - Verify REDIS_LOCAL_* env vars are set
   - Check Redis connection in logs

2. **Still seeing OutOfGas:**
   - Gas history might not be built yet (first run)
   - Check if specific contracts consistently use >100M gas
   - May need to further reduce iterations for those contracts

3. **No profitable opportunities:**
   - This is separate from gas management
   - Check that bounds aren't too restrictive
   - Verify base fee and gas price are set to 0 in simulations

## Expected Behavior

1. **First flashblock:** Full bounds search, may use high gas
2. **Second flashblock:** Adjusted bounds based on first run
3. **Steady state:** Gas usage stabilizes around target (30-40M)
4. **High gas contracts:** Automatically get narrower search bounds
5. **Efficient contracts:** Can search wider bounds

## Key Metrics

- Target gas per scan: 30-40M
- IIR filter α: 0.05 (5% new, 95% old)
- Redis TTL: 1 hour
- Max iterations: 40 (down from 210)