#!/usr/bin/env python3
"""Test the gas price comparison output format."""

# Example values
effective_gas_price = 15000000  # 0.015 gwei
effective_gas_price_at_idx = 25000000  # 0.025 gwei

print(f"\n💰 Gas Price Comparison:")
print(f"  Original tx: {effective_gas_price:,} wei ({effective_gas_price / 1e9:.4f} gwei)")
print(f"  Found tx:    {effective_gas_price_at_idx:,} wei ({effective_gas_price_at_idx / 1e9:.4f} gwei)")
print(f"  Difference:  {effective_gas_price_at_idx - effective_gas_price:,} wei ({(effective_gas_price_at_idx - effective_gas_price) / 1e9:.4f} gwei)")