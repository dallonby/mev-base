#!/usr/bin/env python3

import redis
import time

def test_adaptive_bounds():
    """Test the adaptive bounds adjustment logic"""
    print("Testing Adaptive Bounds Adjustment")
    print("==================================")
    
    # Connect to Redis with authentication
    r = redis.Redis(
        host='localhost', 
        port=6379, 
        password='9PmITfN1LiGZDYNr4Gp0LxU9Dq',
        decode_responses=True
    )
    
    # Test parameters
    target_address = "0x940181a94A35A4569E4529A3CDfB74e38FD98631"
    gas_key = f"mev:gas:{target_address}"
    initial_qty = 1000000  # 1M units
    
    # Simulate different gas consumption scenarios
    scenarios = [
        ("Low gas contract", 10_000_000),    # 10M gas
        ("Medium gas contract", 35_000_000),  # 35M gas  
        ("High gas contract", 60_000_000),    # 60M gas
        ("Very high gas contract", 100_000_000)  # 100M gas
    ]
    
    for name, gas_used in scenarios:
        print(f"\n{name} scenario:")
        print(f"  Gas used: {gas_used:,} ({gas_used/1e6:.0f}M)")
        
        # Calculate bounds adjustment based on target 35M gas
        target_gas = 35_000_000
        
        if gas_used <= target_gas:
            # Within target, can search wider
            factor = min(2.0, target_gas / max(gas_used, 1))
            adjusted_lower = max(1, int(initial_qty / factor))
            adjusted_upper = int(initial_qty * factor)
            print(f"  ✓ Within target gas limit")
        else:
            # Over target, need to narrow search
            factor = gas_used / target_gas
            adjusted_lower = max(1, int(initial_qty / 10))
            adjusted_upper = max(initial_qty, int(initial_qty / factor))
            print(f"  ⚠️  Exceeds target gas limit by {factor:.1f}x")
        
        print(f"  Original bounds: {1:,} - {initial_qty * 10:,}")
        print(f"  Adjusted bounds: {adjusted_lower:,} - {adjusted_upper:,}")
        print(f"  Search range reduction: {100 * (1 - (adjusted_upper - adjusted_lower) / (initial_qty * 10 - 1)):.1f}%")
        
        # Store in Redis for testing
        r.setex(gas_key, 3600, str(gas_used))
        print(f"  Stored in Redis: {gas_key} = {gas_used}")
        
        # Simulate IIR filter update
        if gas_used > target_gas:
            # Second run with reduced quantity would use less gas
            second_run_gas = int(gas_used * 0.6)  # 60% of original
            filtered_gas = int(second_run_gas * 0.05 + gas_used * 0.95)
            print(f"  After optimization:")
            print(f"    Second run gas: {second_run_gas:,} ({second_run_gas/1e6:.0f}M)")
            print(f"    Filtered gas (IIR α=0.05): {filtered_gas:,} ({filtered_gas/1e6:.0f}M)")
            r.setex(gas_key, 3600, str(filtered_gas))

    # Show final state
    print("\n\nFinal gas history in Redis:")
    print("--------------------------")
    pattern = "mev:gas:*"
    for key in r.scan_iter(match=pattern):
        value = r.get(key)
        ttl = r.ttl(key)
        print(f"  {key}: {value} (TTL: {ttl}s)")
    
    print("\n✅ Adaptive bounds test complete!")

if __name__ == "__main__":
    test_adaptive_bounds()