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

## Recent Updates (Latest: 2025-08-01)

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

### Redis Transaction Broadcasting
- Transactions are now broadcast to Redis concurrently with sequencer submission
- Redis channel: `baseTransactionBroadcast` 
- Enables distributed MEV bot network to share transactions
- Handles race conditions gracefully: if sequencer reports "already known" but Redis succeeded, considers it a success
- Redis connection is initialized asynchronously to not block startup

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