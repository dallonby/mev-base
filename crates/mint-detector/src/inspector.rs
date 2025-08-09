use alloy_primitives::{Address, B256, U256};
use revm_inspectors::tracing::{TracingInspector, TracingInspectorConfig, CallTraceArena};
use serde::{Deserialize, Serialize};

/// ERC20 Transfer event topic
pub const ERC20_TRANSFER_TOPIC: B256 = B256::new([
    0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b, 0x69, 0xc2, 0xb0, 0x68,
    0xfc, 0x37, 0x8d, 0xaa, 0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16,
    0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef
]);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc20Transfer {
    pub token: Address,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
    pub log_index: usize,
}

/// Inspector that traces calls and captures ERC20 events
pub struct MintDetectorInspector {
    inner: TracingInspector,
}

impl MintDetectorInspector {
    pub fn new() -> Self {
        let config = TracingInspectorConfig::default_parity();
        Self {
            inner: TracingInspector::new(config),
        }
    }
    
    /// Get the underlying tracing inspector
    pub fn inner(&self) -> &TracingInspector {
        &self.inner
    }
    
    /// Get mutable reference to the underlying tracing inspector
    pub fn inner_mut(&mut self) -> &mut TracingInspector {
        &mut self.inner
    }
    
    /// Get the call trace arena
    pub fn get_traces(&self) -> &CallTraceArena {
        self.inner.traces()
    }
    
    /// Extract all ERC20 transfer events from the trace
    pub fn extract_erc20_transfers(&self) -> Vec<Erc20Transfer> {
        let mut transfers = Vec::new();
        let traces = self.inner.traces();
        let mut log_index = 0;
        
        // Iterate through all nodes in the call trace arena
        for node in traces.nodes().iter() {
            // Get the execution address for this node (the contract that emitted the logs)
            let contract_address = node.execution_address();
            
            // Check logs in this call
            for log in &node.logs {
                let raw_log = &log.raw_log;
                if raw_log.topics().len() == 3 && raw_log.topics()[0] == ERC20_TRANSFER_TOPIC {
                    // This is an ERC20 transfer event
                    let from = Address::from_slice(&raw_log.topics()[1].as_slice()[12..]);
                    let to = Address::from_slice(&raw_log.topics()[2].as_slice()[12..]);
                    let amount = U256::from_be_bytes(raw_log.data.to_vec().try_into().unwrap_or([0u8; 32]));
                    
                    transfers.push(Erc20Transfer {
                        token: contract_address,  // The contract that emitted the event is the token
                        from,
                        to,
                        amount,
                        log_index,
                    });
                    log_index += 1;
                }
            }
        }
        
        transfers
    }
    
    /// Detect potential mint/burn patterns
    pub fn detect_mint_burn_patterns(&self) -> Vec<MintBurnPattern> {
        let transfers = self.extract_erc20_transfers();
        let mut patterns = Vec::new();
        
        // Look for patterns in transfers
        for (i, transfer) in transfers.iter().enumerate() {
            // Check for burn (transfer to zero address or dead address)
            if transfer.to == Address::ZERO || 
               transfer.to == Address::from([0xde, 0xad, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]) {
                // Look for corresponding transfer of different token
                for other in transfers.iter().skip(i + 1) {
                    if other.token != transfer.token && other.to == transfer.from {
                        patterns.push(MintBurnPattern::Burn {
                            burned_token: transfer.token,
                            minted_token: other.token,
                            amount_burned: transfer.amount,
                            amount_minted: other.amount,
                            address: transfer.from,
                        });
                    }
                }
            }
            
            // Check for mint (transfer from zero address)
            if transfer.from == Address::ZERO {
                // Look for corresponding transfer to this mint
                for other in transfers.iter().take(i) {
                    if other.token != transfer.token && other.to == transfer.to {
                        patterns.push(MintBurnPattern::Mint {
                            source_token: other.token,
                            minted_token: transfer.token,
                            amount_source: other.amount,
                            amount_minted: transfer.amount,
                            address: transfer.to,
                        });
                    }
                }
            }
            
            // Check for quantity match pattern (potential synthetic mint)
            for other in transfers.iter().skip(i + 1) {
                if transfer.token != other.token {
                    // Check if amounts are similar (within 15% tolerance)
                    let ratio = if transfer.amount > other.amount {
                        transfer.amount * U256::from(100) / other.amount.max(U256::from(1))
                    } else {
                        other.amount * U256::from(100) / transfer.amount.max(U256::from(1))
                    };
                    
                    if ratio >= U256::from(85) && ratio <= U256::from(115) {
                        patterns.push(MintBurnPattern::QuantityMatch {
                            token_a: transfer.token,
                            token_b: other.token,
                            amount_a: transfer.amount,
                            amount_b: other.amount,
                            similarity_ratio: ratio,
                        });
                    }
                }
            }
        }
        
        patterns
    }
}

impl Default for MintDetectorInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MintBurnPattern {
    Mint {
        source_token: Address,
        minted_token: Address,
        amount_source: U256,
        amount_minted: U256,
        address: Address,
    },
    Burn {
        burned_token: Address,
        minted_token: Address,
        amount_burned: U256,
        amount_minted: U256,
        address: Address,
    },
    QuantityMatch {
        token_a: Address,
        token_b: Address,
        amount_a: U256,
        amount_b: U256,
        similarity_ratio: U256,
    },
}