use alloy_primitives::{U256, B256};
use alloy_consensus::{TxEip1559, TxEnvelope, Signed, Transaction, SignableTransaction};
use alloy_signer_local::PrivateKeySigner;
use alloy_network::TxSigner;
use reth_provider::{StateProviderFactory, HeaderProvider};
use alloy_consensus::BlockHeader;
use alloy_eips::eip2718::Encodable2718;
use eyre::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use rand::Rng;

use crate::mev_search_worker::MevOpportunity;
use crate::wallet_service::WalletService;
use crate::sequencer_service::SequencerService;

/// Configuration for the transaction service
#[derive(Debug, Clone)]
pub struct TransactionServiceConfig {
    pub enabled: bool,
    pub dry_run: bool,
    pub chain_id: u64,
    pub default_gas_limit: Option<u64>,
    pub gas_multiplier: f64,
    pub wallet_strategy: WalletStrategy,
}

#[derive(Debug, Clone)]
pub enum WalletStrategy {
    Default,
    Random,
    RoundRobin,
}

impl Default for TransactionServiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dry_run: false,
            chain_id: 8453, // Base mainnet
            default_gas_limit: None,
            gas_multiplier: 1.2,
            wallet_strategy: WalletStrategy::Default,
        }
    }
}

/// Service for processing MEV opportunities into transactions
pub struct TransactionService {
    config: TransactionServiceConfig,
    wallet_service: Arc<WalletService>,
    sequencer_service: Arc<SequencerService>,
    wallet_index: Arc<RwLock<usize>>,
}

impl TransactionService {
    pub fn new(
        config: TransactionServiceConfig,
        wallet_service: Arc<WalletService>,
        sequencer_service: Arc<SequencerService>,
    ) -> Self {
        Self {
            config,
            wallet_service,
            sequencer_service,
            wallet_index: Arc::new(RwLock::new(0)),
        }
    }

    /// Process an MEV opportunity into a transaction
    /// This is the Rust equivalent of TypeScript's processBuilder method
    pub async fn process_opportunity<P>(
        &self,
        opportunity: &MevOpportunity,
        provider: &P,
    ) -> Result<Option<B256>> 
    where
        P: StateProviderFactory + HeaderProvider + reth_provider::BlockNumReader,
        P::Header: BlockHeader,
    {
        if !self.config.enabled {
            debug!("Transaction service is disabled, skipping opportunity");
            return Ok(None);
        }

        let start_time = std::time::Instant::now();

        // Get wallet for signing
        let wallet = self.get_next_wallet().await?;
        let wallet_address = wallet.address();

        // Get nonce from state provider
        let state = provider.latest()?;
        let account = state.basic_account(&wallet_address)?;
        let nonce = account.map(|acc| acc.nonce).unwrap_or(0);

        info!(
            block = opportunity.block_number,
            flashblock = opportunity.flashblock_index,
            strategy = %opportunity.strategy,
            wallet = %wallet_address,
            nonce = nonce,
            expected_profit = %opportunity.expected_profit,
            simulated_gas_used = ?opportunity.simulated_gas_used,
            bundle_size = opportunity.bundle.transactions.len(),
            "Processing MEV opportunity"
        );

        // Extract transaction details from the opportunity
        // In the TypeScript version, this comes from builder.build(context)
        // Here, we already have the built transaction in the opportunity
        let bundle_tx = opportunity.bundle.transactions.first()
            .ok_or_else(|| eyre::eyre!("No transactions in MEV bundle"))?;

        // Extract transaction parameters based on bundle transaction type
        let (to, data, value) = match bundle_tx {
            crate::mev_bundle_types::BundleTransaction::Unsigned { to, input, value, .. } => {
                let to_addr = to.ok_or_else(|| eyre::eyre!("Missing 'to' address in bundle transaction"))?;
                info!(
                    tx_type = "unsigned",
                    to = %to_addr,
                    value = %value,
                    input_len = input.len(),
                    input_hex = %hex::encode(&input),
                    "Extracted unsigned transaction parameters"
                );
                (to_addr, input.clone(), *value)
            }
            crate::mev_bundle_types::BundleTransaction::Signed(tx_envelope) => {
                // For signed transactions, extract the fields
                let to_addr = tx_envelope.to()
                    .ok_or_else(|| eyre::eyre!("Missing 'to' address in signed transaction"))?;
                let input = tx_envelope.input().clone();
                let value = tx_envelope.value();
                info!(
                    tx_type = "signed",
                    to = %to_addr,
                    value = %value,
                    input_len = input.len(),
                    input_hex = %hex::encode(&input),
                    "Extracted signed transaction parameters"
                );
                (to_addr, input, value)
            }
        };

        // Get current block header for gas estimation
        let latest_block = provider.best_block_number()?;
        let header = provider.header_by_number(latest_block)?
            .ok_or_else(|| eyre::eyre!("Failed to get latest block header"))?;

        // Determine gas limit
        let gas_limit = if let crate::mev_bundle_types::BundleTransaction::Unsigned { gas_limit, .. } = bundle_tx {
            info!(gas_limit = *gas_limit, source = "bundle", "Using gas limit from bundle");
            *gas_limit
        } else if let Some(simulated_gas) = opportunity.simulated_gas_used {
            // Use simulated gas with a buffer
            let buffered_gas = (simulated_gas as f64 * 1.2) as u64;
            info!(
                simulated_gas = simulated_gas,
                gas_limit = buffered_gas,
                source = "simulated_with_buffer",
                "Using simulated gas with 20% buffer"
            );
            buffered_gas
        } else if let Some(default_limit) = self.config.default_gas_limit {
            info!(gas_limit = default_limit, source = "config", "Using default gas limit from config");
            default_limit
        } else {
            // Estimate gas based on the transaction complexity
            // For MEV transactions, we typically need more gas than simple transfers
            let estimated = match data.len() {
                0..=4 => 21_000u64,           // Simple transfer
                5..=100 => 100_000u64,        // Simple contract call
                101..=500 => 200_000u64,      // Complex contract call
                _ => 300_000u64,              // Very complex operation
            };
            info!(
                gas_limit = estimated,
                source = "estimated",
                data_len = data.len(),
                "Estimated gas limit based on calldata size"
            );
            estimated
        };

        // Calculate gas pricing from actual block header
        let base_fee = header.base_fee_per_gas().unwrap_or(1_000_000) as u128;
        
        // Dynamic priority fee: allocate 5% of profit to gas fees
        let base_priority_fee = if let Some(simulated_gas) = opportunity.simulated_gas_used {
            if simulated_gas > 0 {
                // 5% of profit allocated to fees, divided by gas used = priority fee per gas
                let profit_wei = opportunity.expected_profit.as_limbs()[0] as u128;
                let fee_budget = profit_wei * 5 / 100; // 5%
                let priority_per_gas = fee_budget / (simulated_gas as u128);
                
                // Cap at a reasonable maximum (e.g., 1 gwei) to avoid overpaying
                let max_priority = 1_000_000_000u128; // 1 gwei
                let capped_priority = priority_per_gas.min(max_priority);
                
                // Slightly randomize by subtracting up to 25,000 wei
                let mut rng = rand::rng();
                let randomization = rng.random_range(0..=25_000u128);
                capped_priority.saturating_sub(randomization)
            } else {
                5_000u128 // Fallback to 0.005 gwei if no gas estimate
            }
        } else {
            5_000u128 // Fallback to 0.005 gwei if no simulation
        };
        
        // Apply processor-specific multiplier if available
        let priority_fee = if let Some(ref config) = opportunity.processor_config {
            if let Some(multiplier) = config.priority_fee_multiplier {
                // multiplier is in format: 10000 = 1x, 15000 = 1.5x
                (base_priority_fee * multiplier as u128) / 10000
            } else {
                base_priority_fee
            }
        } else {
            base_priority_fee
        };
        
        let multiplier = (self.config.gas_multiplier * 100.0) as u128;
        let max_priority_fee_per_gas = priority_fee;
        let max_fee_per_gas = (base_fee * multiplier / 100) + priority_fee;
        
        info!(
            base_fee_wei = base_fee,
            base_fee_gwei = base_fee as f64 / 1e9,
            base_priority_fee_wei = base_priority_fee,
            base_priority_fee_gwei = base_priority_fee as f64 / 1e9,
            priority_fee_wei = priority_fee,
            priority_fee_gwei = priority_fee as f64 / 1e9,
            processor_multiplier = opportunity.processor_config.as_ref()
                .and_then(|c| c.priority_fee_multiplier)
                .map(|m| format!("{}x", m as f64 / 10000.0))
                .unwrap_or_else(|| "1x (default)".to_string()),
            base_multiplier = self.config.gas_multiplier,
            max_fee_per_gas_wei = max_fee_per_gas,
            max_fee_per_gas_gwei = max_fee_per_gas as f64 / 1e9,
            profit_allocation = "5%",
            simulated_gas = ?opportunity.simulated_gas_used,
            expected_profit_wei = %opportunity.expected_profit,
            "Calculated dynamic gas pricing"
        );

        // Build the transaction
        let tx = TxEip1559 {
            chain_id: self.config.chain_id,
            nonce,
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            to: alloy_primitives::TxKind::Call(to),
            value,
            access_list: Default::default(),
            input: data.clone(),
        };

        info!(
            to = %to,
            value = %value,
            value_hex = %format!("0x{:x}", value),
            gas_limit = gas_limit,
            max_fee_per_gas = max_fee_per_gas,
            max_priority_fee_per_gas = max_priority_fee_per_gas,
            nonce = nonce,
            chain_id = self.config.chain_id,
            input_len = data.len(),
            "Built EIP-1559 transaction"
        );

        // Sign the transaction
        info!("Starting transaction signing");
        let mut tx_mut = tx.clone();
        let signature = wallet.sign_transaction(&mut tx_mut).await?;
        let tx_hash = tx.signature_hash();
        let signed_tx = TxEnvelope::Eip1559(Signed::new_unchecked(tx, signature, tx_hash));
        // Use EIP-2718 encoding for typed transactions
        let signed_bytes = signed_tx.encoded_2718();
        let signed_hex = format!("0x{}", hex::encode(&signed_bytes));

        info!(
            strategy = %opportunity.strategy,
            tx_hash = %signed_tx.tx_hash(),
            signed_size = signed_bytes.len(),
            signed_hex_preview = %format!("{}...{}", 
                &signed_hex[..20.min(signed_hex.len())],
                &signed_hex[signed_hex.len().saturating_sub(20)..]
            ),
            "Signed MEV transaction"
        );

        // Check if dry run mode
        if self.config.dry_run {
            info!("DRY RUN MODE - Not submitting transaction");
            self.log_dry_run(&opportunity, &tx_mut, &signed_hex).await;
            return Ok(None);
        }

        // Submit to sequencer
        info!("Submitting transaction to sequencer");
        match self.sequencer_service.send_transaction(&signed_hex).await {
            Ok(tx_hash) => {
                let elapsed = start_time.elapsed();
                info!(
                    block = opportunity.block_number,
                    flashblock = opportunity.flashblock_index,
                    strategy = %opportunity.strategy,
                    tx_hash = %tx_hash,
                    elapsed_ms = elapsed.as_millis(),
                    expected_profit = %opportunity.expected_profit,
                    "🎯💰 MEV JACKPOT SUBMITTED! 🎰🚀 Profit incoming: {} wei! 💎🔥 Strategy {} STRIKES GOLD! 🏆✨",
                    opportunity.expected_profit,
                    opportunity.strategy
                );
                Ok(Some(tx_hash))
            }
            Err(e) => {
                error!(
                    block = opportunity.block_number,
                    flashblock = opportunity.flashblock_index,
                    strategy = %opportunity.strategy,
                    error = ?e,
                    error_message = %e,
                    "Failed to submit MEV transaction"
                );
                Err(e)
            }
        }
    }

    /// Get the next wallet based on the configured strategy
    async fn get_next_wallet(&self) -> Result<PrivateKeySigner> {
        match self.config.wallet_strategy {
            WalletStrategy::Random => {
                self.wallet_service.get_random_wallet()
            }
            WalletStrategy::RoundRobin => {
                let mut index = self.wallet_index.write().await;
                let wallet = self.wallet_service.get_wallet(*index)?;
                *index = (*index + 1) % self.wallet_service.wallet_count();
                Ok(wallet)
            }
            WalletStrategy::Default => {
                self.wallet_service.get_wallet(0)
            }
        }
    }


    /// Log transaction details in dry run mode
    async fn log_dry_run(
        &self,
        opportunity: &MevOpportunity,
        tx: &TxEip1559,
        signed_hex: &str,
    ) {
        println!("\n{}", "=".repeat(80));
        println!("DRY RUN MODE - Block {} - {}", opportunity.block_number, opportunity.strategy);
        println!("{}", "=".repeat(80));
        println!("WOULD SUBMIT MEV TRANSACTION:");
        println!("  Strategy: {}", opportunity.strategy);
        println!("  To:       {:?}", tx.to);
        println!("  Value:    {} ETH", format_ether(tx.value));
        println!("  Data:     0x{}... ({} bytes)", 
            hex::encode(&tx.input[..10.min(tx.input.len())]), 
            tx.input.len()
        );
        println!("  Gas:      {} units", tx.gas_limit);
        println!("  Max Fee:  {} gwei", tx.max_fee_per_gas as f64 / 1e9);
        println!("  Priority: {} gwei", tx.max_priority_fee_per_gas as f64 / 1e9);
        println!("  Nonce:    {}", tx.nonce);
        println!("\nExpected Profit: {} ETH", format_ether(U256::from(opportunity.expected_profit)));
        println!("\nSigned TX: {}...{}", 
            &signed_hex[..50.min(signed_hex.len())],
            &signed_hex[signed_hex.len().saturating_sub(50)..]
        );
        println!("{}\n", "=".repeat(80));

        info!(
            block = opportunity.block_number,
            strategy = %opportunity.strategy,
            expected_profit = %opportunity.expected_profit,
            "DRY RUN - Would submit MEV transaction"
        );
    }
}

/// Format wei value to ETH string
fn format_ether(wei: U256) -> String {
    let eth = wei.as_limbs()[0] as f64 / 1e18;
    format!("{:.6}", eth)
}