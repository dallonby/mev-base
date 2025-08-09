# Base Mainnet REVM Template

A production-ready template for building Base mainnet applications using REVM (Rust Ethereum Virtual Machine).

## Features

- ✅ **Base Mainnet Configuration**: Pre-configured for Base mainnet (Chain ID 8453)
- ✅ **Optimism Support**: Full support for Optimism/Base-specific features via `op-revm`
- ✅ **REVM Integration**: Direct EVM execution with state management
- ✅ **Transaction Support**: EIP-1559 transaction creation and execution
- ✅ **State Management**: CacheDB for efficient state tracking

## Quick Start

```bash
cargo run --release
```

## Architecture

### Core Components

1. **OpEvmConfig**: Configures the EVM for Base/Optimism execution
2. **CacheDB**: In-memory state database with caching capabilities
3. **OpTransaction**: Optimism-specific transaction wrapper
4. **EvmEnv**: Environment configuration for EVM execution

### Key Dependencies

- `revm`: Core EVM implementation
- `op-revm`: Optimism/Base-specific EVM extensions
- `reth-optimism-*`: Reth's Optimism/Base support libraries
- `alloy-*`: Ethereum data types and utilities

## Usage Examples

### Basic Transaction Execution

The template includes a working example of:
1. Setting up a CacheDB with test accounts
2. Configuring the EVM for Base mainnet
3. Creating and signing an EIP-1559 transaction
4. Executing the transaction in the EVM
5. Processing the execution results

### Extending the Template

To build your own application:

1. **State Access**: Replace the `EmptyDB` with a real database connection
2. **Custom Logic**: Add your business logic in the transaction processing
3. **Inspector Integration**: Add custom inspectors for transaction analysis
4. **Block Processing**: Extend to process entire blocks of transactions

## Base Mainnet Specifics

### Chain Configuration
- Chain ID: 8453
- Typical Base Fee: 0.05 gwei
- Gas Limit: 30,000,000 per block

### Important Considerations
- Base uses Optimism's transaction format
- All non-deposit transactions require proper envelope encoding
- The EVM must be configured with OpEvmConfig for correct execution

## Development Tips

1. **Always use proper types**: Use `OpPrimitives` and `OpChainSpec` type parameters
2. **Transaction envelopes**: All transactions need proper EIP-2718 encoding
3. **State management**: Use CacheDB for efficient state tracking during execution
4. **Error handling**: The template uses `eyre::Result` for comprehensive error handling

## Common Patterns

### Reading from Reth Database
```rust
// Replace EmptyDB with actual database
use reth_provider::{DatabaseProvider, ProviderFactory};
let provider = ProviderFactory::new(db_path)?;
let state_provider = provider.latest()?;
```

### Custom Inspector
```rust
use revm::Inspector;
struct MyInspector;
impl Inspector for MyInspector {
    // Implement inspection methods
}
```

### Block Processing
```rust
// Process transactions from a block
for tx in block.transactions {
    let result = evm.transact(tx)?;
    // Process result
}
```

## Testing

Run tests with:
```bash
cargo test
```

## License

MIT OR Apache-2.0 (same as the workspace)