mandatory: must use Serena for any file operations if it all possible

## Cargo Commands

- NEVER USE `2>&1` in command line for cargo
- build with `cargo build -p mevbase --profile=release`

## Cast Commands

- Always use `--rpc-url /tmp/op-reth` for cast commands to use the local IPC connection

## PRIME DIRECTIVES
- No mocks. No stubs. No simplifications. Create code that works for productions.
- Use `cargo build` to check for compilation errors. Do it OFTEN. Don't add functionality until the code compiles.
- Source code for reth, alloy, and rollup-boost is in ~/.cargo/git/checkouts/.  Use grep to find the source code for a function or struct.
- Document changes to architecture, design, and interfaces into CLAUDE.md
- Refer to this document every time you /compact
- Compile OFTEN.

## Recent Updates (Latest: 2025-08-04)

### Worker Timeout Protection
- Added timeout protection to MEV worker tasks to prevent stuck database transactions
- Configurable via `MEV_WORKER_TIMEOUT_SECS` environment variable (default: 30 seconds)
- When a worker times out:
  - Database transaction is automatically released via RAII
  - Error logged with timeout duration and strategy name
  - System continues processing other opportunities
- Prevents database connection exhaustion from stuck workers

### Logging System
- Implemented structured logging using `tracing` crate
- Configurable verbosity via environment variables:
  - `MEV_LOG`: Primary configuration (e.g., "debug", "info", "warn", "error", "trace")
  - `RUST_LOG`: Fallback if MEV_LOG not set
- Features:
  - Structured fields for block numbers, flashblock indices, profits, etc.
  - Module-specific filtering (e.g., `MEV_LOG=info,mevbase::mev_task_worker=debug`)
  - Compact output format with timestamps and log levels
  - Supports .env file configuration
- Usage: Set `MEV_LOG=debug` for detailed execution logs

### Backtest Tools
- **mev-analyze**: Analyze historical MEV results from JSON logs
  - Filter by block, flashblock, strategy
  - Show statistics and profitability analysis
  - Usage: `mev-analyze --results-file mev_results.jsonl --stats`
  - Example filtering: `mev-analyze --results-file mev_results.jsonl --block 33646198 --strategy Backrun_AeroWeth`

- **mev-backtest-block**: Replay block transactions to find MEV opportunities
  - Connects to running node via RPC (default: http://localhost:8545)
  - Can analyze after specific transaction index or all transactions
  - Filter by specific processor configuration
  - Usage: `mev-backtest-block --block 33646198 --processor AeroWeth --rpc http://localhost:8545`
  - With IPC: `mev-backtest-block --block 33646198 --rpc ~/.local/share/reth/mainnet/reth.ipc`
  - Note: Requires running Base node with RPC/IPC enabled

### Backtest Architecture Notes
- Uses alloy-provider for RPC/IPC connection
- Creates mock state snapshots (full REVM integration pending)
- Runs backrun analyzer at each transaction index
- Can filter for specific processor configurations
- Future: Full transaction replay with REVM state tracking

### MEV Profit Threshold & Logging
- Minimum profit threshold updated to 0.00001 ETH (10 microether)
  - Accounts for Base mainnet's ultra-low gas costs (0.01 gwei)
  - Ensures 5x profit margin over typical 200k gas transaction (~0.000002 ETH)
- JSON logging to `mev_results.jsonl` for opportunities exceeding threshold
  - Logs: timestamp, block/flashblock, strategy, profit (wei & ETH), bundle details
  - Only logs from main MEV handler after receiving results from workers

### Performance Optimizations
- **Batch Task Spawning**: Reduced latency from 9.25ms to ~3.5ms
  - All MEV tasks spawn together using `spawn_mev_tasks_batch()`
  - State snapshot wrapped in Arc to avoid multiple clones
  - Parallel execution with minimal overhead
- **FastGradientOptimizer**: 10x speedup (900ms → 90ms)
  - Reduced iterations from 250 to 50
  - Pre-funds bot address once
  - Reuses EVM environment
- **Dynamic Bounds**: min = max(1, initialQty/100), max = min(initialQty*100, 0xffffff)

### Code Cleanup
- Removed mock strategies (DexArbitrage, Liquidation, Sandwich, JitLiquidity)
- Only production Backrun strategy remains
- Removed unused gradient logging code

### Dynamic Priority Fee
- MEV bots now allocate 15% of expected profit to gas fees (updated from 5%)
- Priority fee = (expected_profit * 15%) / simulated_gas_used
- Capped at 1 gwei maximum to prevent overpaying
- Includes slight randomization (subtract 0-25k wei) to avoid detection patterns

### MEV Results Monitor
- **monitor_mev_results.py**: Real-time monitoring of MEV transaction outcomes
  - Watches `mev_results.jsonl` for new entries
  - Checks if our transactions made it on-chain using cast
  - For failed transactions, analyzes who beat us:
    - Runs `find_eth_transfer_point.py` to identify winning transaction
    - Shows competitor's tx hash, gas price, and sender
    - Indicates if we were in same flashblock (gas price issue) or different (timing issue)
  - Usage:
    ```bash
    # Monitor continuously
    scripts/monitor_mev.sh
    
    # Analyze last N entries
    python3 scripts/monitor_mev_results.py --last 10
    
    # Run once on new entries
    python3 scripts/monitor_mev_results.py --once
    ```
  - Output shows:
    - ✅ Success: Transaction included and executed
    - ❌ Reverted: Transaction included but failed
    - ❌ Not Included: Lost to competitor (with detailed analysis)

### Adaptive Gas Management
- Gas history stored in Redis with 24-hour TTL
- IIR filter with α=0.05 for smooth gas tracking
- Target gas scales with iterations: 875k gas per iteration (35M for 40 iterations)
- To increase iterations: `export MEV_GRADIENT_ITERATIONS=60`
- Bounds adjustment:
  - 2x over target: reduce to 50%
  - Over target: reduce to 80%
  - Under half target: increase to 150%
  - Otherwise: keep as is

### Redis Transaction Broadcasting (DISABLED)
- **Status: Temporarily disabled** - Was causing issues with transaction inclusion
- Previously broadcast to Redis channel: `baseTransactionBroadcast`
- Code is commented out in `sequencer_service.rs` but can be re-enabled if needed
- Original purpose: Enable distributed MEV bot network to share transactions

## Architecture Updates

### Parallel Backrun Workers
- Each triggered backrun config spawns its own worker thread with independent CacheDB
- Workers run in parallel, each optimizing a different MEV opportunity
- Example: When 4 configs trigger (UsdbcWethUsdc, AeroWeth, UsdcUsdtWeth, WethUsdc), 4 parallel workers spawn

### Strategy Detection (Currently Disabled)
- Oracle update detection code is commented out but preserved for future liquidation implementation
- Function selectors for oracle updates:
  - `0x50d25bcd`: latestAnswer()
  - `0x9a6fc8f5`: transmit()
  - `0xc9807539`: submit()
  - `0x6fadcf72`: forward()
- This code will be re-enabled when implementing Aave V3 liquidation logic

### Performance Optimizations
- FastGradientOptimizer achieves 10x speedup (900ms → 90ms)
- Reduced iterations from 250 to 50
- Pre-funds bot address once instead of per-iteration
- Streamlined EVM execution with reused environments
