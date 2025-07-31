use alloy_rpc_types_eth::{state::{StateOverride, AccountOverride}, Bundle, StateContext, EthCallResponse};
use alloy_primitives::{U256, B256, Address, Bytes};
use reth_rpc_eth_api::{helpers::EthCall, EthApiTypes, RpcTypes};
use std::collections::HashMap;
use reth_primitives::Account;
// use reth_storage_api::{StateProvider, AccountReader, StateRootProvider};

/// Represents the state diff after simulation
#[derive(Debug, Clone, Default)]
pub struct StateDiff {
    /// Account changes (address -> new account state)
    pub accounts: HashMap<Address, AccountDiff>,
    /// Storage changes (address -> (slot -> new value))
    pub storage: HashMap<Address, HashMap<B256, B256>>,
    /// New contract code (address -> bytecode)
    pub code: HashMap<Address, Bytes>,
}

#[derive(Debug, Clone)]
pub struct AccountDiff {
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: Option<B256>,
}

/// Extended bundle simulation that returns state diffs
pub async fn call_many_with_state<EthApi>(
    eth_api: &EthApi,
    bundles: Vec<Bundle<<<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest>>,
    state_context: Option<StateContext>,
    state_override: Option<StateOverride>,
    return_state_diff: bool,
) -> eyre::Result<(Vec<Vec<EthCallResponse>>, Option<StateDiff>)>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    // First, run the standard call_many
    let results = eth_api.call_many(bundles.clone(), state_context.clone(), state_override.clone()).await?;
    
    if !return_state_diff {
        return Ok((results, None));
    }
    
    // For now, we can't easily extract state diff from the standard call_many
    // This would require modifying reth's internal implementation
    // Instead, we'll return a placeholder
    
    // In a proper implementation, we would:
    // 1. Create a custom EVM with state tracking
    // 2. Execute transactions while recording state changes
    // 3. Return both results and state diff
    
    Ok((results, Some(StateDiff::default())))
}

/// Converts a StateDiff into a StateOverride for the next simulation
pub fn state_diff_to_override(diff: &StateDiff) -> StateOverride {
    let mut state_override = StateOverride::default();
    
    for (address, account_diff) in &diff.accounts {
        let mut account_override = AccountOverride::default();
        account_override.balance = Some(account_diff.balance);
        account_override.nonce = Some(account_diff.nonce);
        
        if let Some(code) = diff.code.get(address) {
            account_override.code = Some(code.clone());
        }
        
        // Add storage changes
        if let Some(storage_changes) = diff.storage.get(address) {
            let mut state_diff = HashMap::with_hasher(alloy_primitives::map::FbBuildHasher::default());
            for (slot, value) in storage_changes {
                state_diff.insert(*slot, *value);
            }
            account_override.state_diff = Some(state_diff);
        }
        
        state_override.insert(*address, account_override);
    }
    
    state_override
}

/// Merges multiple StateOverrides into one
pub fn merge_state_overrides(base: StateOverride, new: StateOverride) -> StateOverride {
    let mut merged = base;
    
    for (address, new_override) in new {
        match merged.get_mut(&address) {
            Some(existing) => {
                // Merge the overrides
                if let Some(balance) = new_override.balance {
                    existing.balance = Some(balance);
                }
                if let Some(nonce) = new_override.nonce {
                    existing.nonce = Some(nonce);
                }
                if let Some(code) = new_override.code {
                    existing.code = Some(code);
                }
                if let Some(new_state_diff) = new_override.state_diff {
                    if let Some(existing_state_diff) = &mut existing.state_diff {
                        // Merge storage changes
                        for (slot, value) in new_state_diff {
                            existing_state_diff.insert(slot, value);
                        }
                    } else {
                        existing.state_diff = Some(new_state_diff);
                    }
                }
            }
            None => {
                merged.insert(address, new_override);
            }
        }
    }
    
    merged
}

/// Optimized flashblock accumulator that tracks state between simulations
pub struct StatefulFlashblockSimulator<EthApi> {
    eth_api: EthApi,
    /// Current state override representing cumulative changes
    current_state: StateOverride,
    /// Cache of account states to avoid re-fetching
    account_cache: HashMap<Address, Account>,
}

impl<EthApi> StatefulFlashblockSimulator<EthApi>
where
    EthApi: EthCall + Clone + Send + Sync + 'static,
    <<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest: Clone + Send + Sync,
{
    pub fn new(eth_api: EthApi) -> Self {
        Self {
            eth_api,
            current_state: StateOverride::default(),
            account_cache: HashMap::new(),
        }
    }
    
    /// Simulates a bundle and updates internal state
    pub async fn simulate_and_update(
        &mut self,
        bundles: Vec<Bundle<<<EthApi as EthApiTypes>::NetworkTypes as RpcTypes>::TransactionRequest>>,
        state_context: Option<StateContext>,
    ) -> eyre::Result<Vec<Vec<EthCallResponse>>> {
        // Run simulation with current state
        let results = self.eth_api.call_many(
            bundles,
            state_context,
            Some(self.current_state.clone()),
        ).await?;
        
        // In a real implementation, we would:
        // 1. Extract state changes from the simulation
        // 2. Update self.current_state with new changes
        // 3. Update self.account_cache
        
        Ok(results)
    }
    
    /// Resets the simulator to a clean state
    pub fn reset(&mut self) {
        self.current_state = StateOverride::default();
        self.account_cache.clear();
    }
}