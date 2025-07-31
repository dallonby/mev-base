## Project Structure

### Source Locations
- Main project source: `/home/ubuntu/Source/reth-mev-standalone/`
- Primary crate: `crates/mev-base/src/`
  - `main.rs` - Entry point, node setup, MEV system initialization
  - `flashblocks.rs` - WebSocket client for flashblocks connection
  - `flashblock_state.rs` - State snapshot structure for MEV searches
  - `revm_flashblock_executor.rs` - Stateful flashblock execution using revm/CacheDB
  - `mev_search_worker.rs` - Work-stealing MEV search system with strategy routing
  - `mev_bundle_types.rs` - MEV bundle and transaction types
  - `mev_simulation.rs` - MEV bundle simulation on flashblock state

### External Dependencies
- Reth source (for reference): `~/.cargo/git/checkouts/reth-e231042ee7db3fb7/`
- Rollup-boost source: `~/.cargo/git/checkouts/rollup-boost-*/`
- Alloy types: `~/.cargo/registry/src/index.crates.io-*/alloy-*`

### Key External Projects
- `node-reth` (reference implementation): `../node-reth/`
  - Contains flashblocks-rpc crate with MEV implementation examples
  - Located at: `/home/ubuntu/Source/node-reth/`

## Architecture Notes

### Flashblocks Integration
- WebSocket endpoint: `wss://mainnet.flashblocks.base.org/ws`
- Receives 11 flashblocks per block (indices 0-10)
- Each flashblock arrives approximately every 200ms
- Flashblocks contain transactions that will be included in the next block
- Each flashblock event includes a timestamp for latency tracking

### Simulation Architecture
- Dedicated synchronous "flashblock-simulator" thread processes flashblocks in order
- Uses `RevmFlashblockExecutor` with CacheDB for stateful simulation
- Simulations run against `BlockId::latest()` (the last finalized block)
- State accumulates across flashblocks within same block
- Execution time: ~0.3-1.8ms per transaction
- State resets when new block is detected

### MEV Search System
- Work-stealing queue architecture optimized for high core counts
- Creates ~102 workers on 128-core machine (80% of available cores)
- Strategy-specific task distribution based on state analysis
- Pipeline latency: typically 6-12ms from flashblock arrival to MEV search start
- Performance metrics tracked throughout pipeline:
  - Flashblock execution time
  - State export time
  - Total latency to MEV search start

### State Export and Analysis
- CacheDB state is exported after each flashblock execution
- Exports include:
  - Account balance/nonce changes
  - Storage slot modifications
  - New contract deployments
  - Original transactions for calldata analysis
- Three-layer strategy triggering:
  1. **Address analysis**: Known DEX/protocol addresses touched
  2. **Storage analysis**: Storage slots modified
  3. **Calldata analysis**: Function selectors called

### MEV Strategy Triggering
- **DexArbitrage**: Triggered by:
  - DEX address touches (Uniswap, BaseSwap, Aerodrome, Sushi)
  - Swap function calldata (swap, multicall)
- **Liquidation**: Triggered by:
  - Chainlink oracle updates (latestAnswer, transmit, submit, forward)
- **Sandwich**: Available but not actively triggered
- **JitLiquidity**: Placeholder for future implementation

### Known Protocol Addresses (Base)
- Uniswap V3: 
  - Factory: `0x33128a8fC17869897dcE68Ed026d694621f6FDfD`
  - Router: `0x2626664c2603336E57B271c5C0b26F421741e481`
- BaseSwap (Uniswap V2 fork):
  - Factory: `0xFDa619b6d20975be80A10332cD39b9a4b0FAa8BB`
  - Router: `0x327Df1E6de05895d2ab08513aaDD9313Fe505d86`
- Aerodrome: Router `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43`
- SushiSwap V3: Factory `0xb45e53277a7e0F1D35f2a77160e91e25507f1763`

### Function Selectors
- Chainlink Oracle:
  - `0x50d25bcd`: latestAnswer()
  - `0x9a6fc8f5`: transmit()
  - `0xc9807539`: submit()
  - `0x6fadcf72`: forward()
- DEX Swaps:
  - `0x022c0d9f`: Uniswap V2 swap()
  - `0x128acab4`: Uniswap V3 swap()
  - `0xac9650d8`: multicall()

### Type Conversions
- Transaction request types require conversion between standard and API-specific types
- Current workaround uses JSON serialization/deserialization (marked as technical debt)

## Cargo Commands

- NEVER USE `2>&1` in command line for cargo

## PRIME DIRECTIVES
- No mocks. No stubs. No simplifications. Create code that works for productions.
- Use `cargo build` to check for compilation errors. Do it OFTEN. Don't add functionality until the code compiles.
- Source code for reth, alloy, and rollup-boost is in ~/.cargo/git/checkouts/.  Use grep to find the source code for a function or struct.
- Document changes to architecture, design, and interfaces into CLAUDE.md
- Refer to this document every time you /compact
- Compile OFTEN.

