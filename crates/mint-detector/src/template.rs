use alloy_primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};

/// Placeholders for template generation
pub struct Placeholders;

impl Placeholders {
    pub const SIMILAR_TO: &'static str = "1b1b1b1b1b1b1b1b2b2b2b2b5b1b1b1b";
    pub const SIMILAR_FROM: &'static str = "1c1c1c1c1c1c1c1c2c2c2c2c5c1c1c1c";
    pub const CALL_FROM: &'static str = "1414141414141414141414142424242424141414";
    pub const CALL_TO: &'static str = "1313131313131313131313132323232333131313";
    pub const TX_SENDER: &'static str = "1212121212121212121212122222222212121212";
    pub const QUANT_FROM: &'static str = "16161616161616162626262646161616";
    pub const QUANT_TO: &'static str = "15151515151515152525252555151515";
    pub const OTHER_EVENT_FROM: &'static str = "1717171717171717171717172727272747171717";
    pub const OTHER_EVENT_TO: &'static str = "1818181818181818181818182828282858181818";
    pub const EVENT_FROM: &'static str = "1919191919191919191919192929292969191919";
    pub const EVENT_TO: &'static str = "1a1a1a1a1a1a1a1a1a1a1a1a2a2a2a2a7a1a1a1a";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintTemplate {
    pub from_token: Address,
    pub to_token: Address,
    pub contract: Address,
    pub calldata_template: String,
    pub original_calldata: Bytes,
    pub from_symbol: Option<String>,
    pub to_symbol: Option<String>,
    pub example_tx: Option<String>,
    pub mint_type: String,
}

pub struct TemplateGenerator;

impl TemplateGenerator {
    /// Generate a template from calldata by replacing variable parts with placeholders
    pub fn generate_template(
        calldata: &Bytes,
        tx_sender: Address,
        call_from: Address,
        call_to: Address,
        quant_from: U256,
        quant_to: U256,
        event_from: Address,
        event_to: Address,
        other_event_from: Option<Address>,
        other_event_to: Option<Address>,
    ) -> String {
        let mut template = hex::encode(calldata);
        
        // Remove 0x prefix if present
        if template.starts_with("0x") {
            template = template[2..].to_string();
        }
        
        // Skip function selector (first 8 chars)
        let selector = template[0..8].to_string();
        let mut params = template[8..].to_string();
        
        // Replace addresses
        params = params.replace(
            &hex::encode(tx_sender.as_slice())[24..],
            Placeholders::TX_SENDER,
        );
        params = params.replace(
            &hex::encode(call_from.as_slice())[24..],
            Placeholders::CALL_FROM,
        );
        params = params.replace(
            &hex::encode(call_to.as_slice())[24..],
            Placeholders::CALL_TO,
        );
        params = params.replace(
            &hex::encode(event_from.as_slice())[24..],
            Placeholders::EVENT_FROM,
        );
        params = params.replace(
            &hex::encode(event_to.as_slice())[24..],
            Placeholders::EVENT_TO,
        );
        
        if let Some(addr) = other_event_from {
            params = params.replace(
                &hex::encode(addr.as_slice())[24..],
                Placeholders::OTHER_EVENT_FROM,
            );
        }
        
        if let Some(addr) = other_event_to {
            params = params.replace(
                &hex::encode(addr.as_slice())[24..],
                Placeholders::OTHER_EVENT_TO,
            );
        }
        
        // Replace quantities
        let quant_from_hex = format!("{:064x}", quant_from);
        let quant_to_hex = format!("{:064x}", quant_to);
        
        params = params.replace(&quant_from_hex, Placeholders::QUANT_FROM);
        params = params.replace(&quant_to_hex, Placeholders::QUANT_TO);
        
        // Look for similar quantities (within 15% range) and replace them
        let chunks = Self::split_into_32byte_chunks(&params);
        let mut modified_params = params.clone();
        
        for chunk in chunks {
            if Self::is_similar_quantity(&chunk, quant_from) {
                modified_params = modified_params.replace(&chunk, Placeholders::SIMILAR_FROM);
            } else if Self::is_similar_quantity(&chunk, quant_to) {
                modified_params = modified_params.replace(&chunk, Placeholders::SIMILAR_TO);
            }
        }
        
        format!("0x{}{}", selector, modified_params)
    }
    
    /// Split calldata into 32-byte chunks (excluding function selector)
    fn split_into_32byte_chunks(data: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut i = 0;
        
        while i < data.len() {
            let end = std::cmp::min(i + 64, data.len());
            chunks.push(data[i..end].to_string());
            i = end;
        }
        
        chunks
    }
    
    /// Check if a hex string represents a quantity similar to the target
    fn is_similar_quantity(hex_str: &str, target: U256) -> bool {
        // Only check full 32-byte values
        if hex_str.len() != 64 {
            return false;
        }
        
        // Parse the hex string as U256
        let value = match U256::from_str_radix(hex_str, 16) {
            Ok(v) => v,
            Err(_) => return false,
        };
        
        // Check if within 15% range
        let lower = target * U256::from(85) / U256::from(100);
        let upper = target * U256::from(115) / U256::from(100);
        
        value >= lower && value <= upper
    }
    
    /// Reconstruct calldata from template by filling in actual values
    pub fn fill_template(
        template: &str,
        our_address: Address,
        quant_in: U256,
    ) -> Bytes {
        let mut filled = template.to_string();
        
        // Remove 0x prefix if present
        if filled.starts_with("0x") {
            filled = filled[2..].to_string();
        }
        
        let our_addr_hex = hex::encode(our_address.as_slice())[24..].to_string();
        let quant_hex = format!("{:064x}", quant_in);
        
        // Replace placeholders with actual values
        filled = filled.replace(Placeholders::TX_SENDER, &our_addr_hex);
        filled = filled.replace(Placeholders::CALL_FROM, &our_addr_hex);
        filled = filled.replace(Placeholders::EVENT_FROM, &our_addr_hex);
        filled = filled.replace(Placeholders::EVENT_TO, &our_addr_hex);
        filled = filled.replace(Placeholders::OTHER_EVENT_FROM, &our_addr_hex);
        filled = filled.replace(Placeholders::OTHER_EVENT_TO, &our_addr_hex);
        filled = filled.replace(Placeholders::QUANT_FROM, &quant_hex);
        filled = filled.replace(Placeholders::QUANT_TO, &quant_hex);
        filled = filled.replace(Placeholders::SIMILAR_FROM, &quant_hex);
        filled = filled.replace(Placeholders::SIMILAR_TO, &quant_hex);
        
        // For CALL_TO, keep the original target
        // This would need to be handled based on the specific use case
        
        Bytes::from(hex::decode(filled).unwrap_or_default())
    }
}