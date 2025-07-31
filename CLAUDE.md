## Project Structure

### Source Locations
- Main project source: `/home/ubuntu/Source/reth-mev-standalone/`
- Primary crate: `crates/mev-base/src/`
  - `main.rs` - Entry point, node setup, flashblocks WebSocket client
  - `flashblocks.rs` - WebSocket connection to wss://mainnet.flashblocks.base.org/ws
  - `flashblock_accumulator.rs` - Manages flashblock state accumulation and incremental simulation
  - `simulation.rs` - Bundle simulation using eth_callMany

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

### Simulation Architecture
- Dedicated synchronous "flashblock-simulator" thread processes flashblocks in order
- Simulations run against `BlockId::latest()` (the last finalized block)
- Cumulative state is tracked using `StateOverride` for each flashblock
- Transaction hashes are displayed in simulation output for tracking

### Type Conversions
- Transaction request types require conversion between standard and API-specific types
- Current workaround uses JSON serialization/deserialization (marked as technical debt)

## Cargo Commands

- NEVER USE `2>&1` in command line for cargo