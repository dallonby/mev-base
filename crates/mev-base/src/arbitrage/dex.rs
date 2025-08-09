use alloy_primitives::{Address, U256};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DexProtocol {
    UniswapV2,
    UniswapV3,
    UniswapV4,
    Aerodrome,
}

impl fmt::Display for DexProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DexProtocol::UniswapV2 => write!(f, "UniswapV2"),
            DexProtocol::UniswapV3 => write!(f, "UniswapV3"),
            DexProtocol::UniswapV4 => write!(f, "UniswapV4"),
            DexProtocol::Aerodrome => write!(f, "Aerodrome"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DexPool {
    pub protocol: DexProtocol,
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub reserve0: U256,
    pub reserve1: U256,
    pub fee: u32, // in basis points (e.g., 30 = 0.3%)
    pub tick: Option<i32>, // For V3/V4
    pub liquidity: Option<U256>, // For V3/V4
}

#[derive(Clone, Debug)]
pub struct SwapRoute {
    pub pools: Vec<DexPool>,
    pub token_path: Vec<Address>,
    pub amount_in: U256,
    pub expected_out: U256,
    pub gas_estimate: u64,
}