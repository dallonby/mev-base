# Proposal: Extending eth_callMany for Stateful Simulation

## Problem Statement

Current `eth_callMany` implementation has a performance bottleneck when simulating sequential flashblocks:
- Each call starts from the base block state
- No state persistence between calls
- 20-25ms per flashblock simulation
- Cumulative overhead for 11 flashblocks per block

## Proposed Solution

Extend `eth_callMany` with a new variant that returns state diffs and accepts initial state:

```rust
/// Extended call_many that supports stateful execution
fn call_many_stateful(
    &self,
    bundles: Vec<Bundle>,
    state_context: Option<StateContext>,
    state_override: Option<StateOverride>,
    initial_state: Option<CachedState>,  // NEW: Accept cached state
) -> impl Future<Output = Result<(
    Vec<Vec<EthCallResponse>>, 
    CachedState  // NEW: Return final state
), Self::Error>>
```

## Implementation Approach

### Option 1: Minimal Change to Existing Code

Add a new RPC method alongside existing `eth_callMany`:

```rust
// In reth/crates/rpc/rpc-eth-api/src/helpers/call.rs
fn call_many_stateful(...) {
    // Reuse existing call_many logic but:
    // 1. Start with provided CachedState instead of fetching from DB
    // 2. Track state changes during execution
    // 3. Return final state
}
```

### Option 2: Add Optional Return Parameter

Modify existing `call_many` to optionally return state:

```rust
fn call_many(
    &self,
    bundles: Vec<Bundle>,
    state_context: Option<StateContext>,
    state_override: Option<StateOverride>,
    return_state: bool,  // NEW: Optional flag
) -> impl Future<Output = Result<CallManyResult, Self::Error>>

enum CallManyResult {
    Simple(Vec<Vec<EthCallResponse>>),
    WithState {
        responses: Vec<Vec<EthCallResponse>>,
        final_state: CachedState,
    }
}
```

### Option 3: Custom ExEx Implementation

Implement stateful simulation entirely in the ExEx without modifying reth core:

```rust
// In our codebase
struct StatefulSimulator {
    // Direct access to state DB
    state_provider: Arc<dyn StateProvider>,
    // In-memory state cache
    state_cache: HashMap<Address, AccountState>,
    // Custom EVM instance
    evm: EVM<CachedDB>,
}

impl StatefulSimulator {
    async fn simulate_bundle(&mut self, bundle: Bundle) -> Result<Vec<EthCallResponse>> {
        // Use cached state, execute, update cache
    }
}
```

## Performance Benefits

1. **Eliminate State Fetching**: ~5-10ms saved per simulation
2. **Reduce RPC Overhead**: Direct EVM access
3. **State Caching**: Reuse hot account data
4. **Batch Processing**: Process multiple flashblocks together

Expected improvement: 20-25ms → 5-10ms per flashblock

## Integration Points

### Minimal Changes Required:

1. **Add new trait method** in `EthCall` trait
2. **Implement in EthApi** with state tracking
3. **Export state diff types** for serialization
4. **Add RPC endpoint** (optional)

### Key Files to Modify:

```
reth/crates/rpc/rpc-eth-api/src/helpers/call.rs
reth/crates/rpc/rpc-eth-api/src/traits.rs
reth/crates/rpc/rpc-eth-types/src/call.rs
```

## Alternative: Local Implementation

If modifying reth core is not feasible, implement locally:

```rust
// Use revm directly with cached state
let mut cached_db = CachedDB::new(state_provider);
let mut evm = EVM::new(cached_db, env);

// Execute transactions and track state
for tx in bundle {
    evm.transact(tx)?;
    // State automatically cached in CachedDB
}

// Reuse cached_db for next bundle
```

## Next Steps

1. **Benchmark current bottlenecks** to confirm state fetching overhead
2. **Prototype Option 3** (local implementation) for immediate gains
3. **Submit RFC to reth** for Option 1 or 2 if successful
4. **Implement state diff extraction** from EVM execution

## Code Example: Minimal Diff Approach

```rust
// New file: reth/crates/rpc/rpc-eth-api/src/helpers/stateful_call.rs
pub trait StatefulEthCall: EthCall {
    fn call_many_stateful(
        &self,
        bundles: Vec<Bundle>,
        state_context: Option<StateContext>,
        cached_state: Option<Arc<CachedState>>,
    ) -> impl Future<Output = Result<(Vec<Vec<EthCallResponse>>, Arc<CachedState>), Self::Error>> {
        // Implementation
    }
}

// Minimal change to existing code:
// Just add state tracking to existing execution flow
```

This approach minimizes changes to reth core while providing the performance benefits we need.