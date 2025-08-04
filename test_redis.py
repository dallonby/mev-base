#!/usr/bin/env python3
import os
import redis
import time

def test_redis():
    # Get connection details from environment
    redis_host = os.environ.get('REDIS_LOCAL_HOST', os.environ.get('REDIS_HOST', 'localhost'))
    redis_port = int(os.environ.get('REDIS_LOCAL_PORT', os.environ.get('REDIS_PORT', '6379')))
    redis_password = os.environ.get('REDIS_LOCAL_PASSWORD', os.environ.get('REDIS_PASSWORD', ''))
    
    print(f"Testing Redis connection to {redis_host}:{redis_port}")
    
    try:
        # Create Redis connection
        if redis_password:
            r = redis.Redis(host=redis_host, port=redis_port, password=redis_password, decode_responses=True)
        else:
            r = redis.Redis(host=redis_host, port=redis_port, decode_responses=True)
        
        # Test 1: Ping
        print("\nTest 1: Ping")
        pong = r.ping()
        print(f"  Ping response: {pong}")
        
        # Test 2: Basic set/get
        print("\nTest 2: Basic set/get")
        key = "test:mev:simple"
        value = "Hello MEV!"
        r.set(key, value)
        print(f"  Set '{key}' = '{value}'")
        
        result = r.get(key)
        print(f"  Get '{key}' = '{result}'")
        assert result == value
        print("  ✓ Basic set/get works!")
        
        # Test 3: Set with TTL
        print("\nTest 3: Set with TTL")
        gas_key = "mev:gas:0x1234567890123456789012345678901234567890"
        gas_value = "35000000"  # 35M gas
        
        r.setex(gas_key, 3600, gas_value)  # 1 hour TTL
        print(f"  Set '{gas_key}' = '{gas_value}' with 1 hour TTL")
        
        ttl = r.ttl(gas_key)
        print(f"  TTL remaining: {ttl} seconds")
        assert 3590 < ttl <= 3600
        print("  ✓ TTL works correctly!")
        
        # Test 4: Gas history workflow
        print("\nTest 4: Gas history workflow")
        target_address = "0x940181a94A35A4569E4529A3CDfB74e38FD98631"
        gas_history_key = f"mev:gas:{target_address}"
        
        # First run - no history
        initial_gas = r.get(gas_history_key)
        print(f"  Initial gas history: {initial_gas}")
        
        # Simulate first run with 50M gas
        first_run_gas = 50_000_000
        r.setex(gas_history_key, 3600, str(first_run_gas))
        print(f"  First run gas: {first_run_gas} (50M)")
        
        # Get filtered value for second run
        stored_gas = int(r.get(gas_history_key))
        print(f"  Retrieved gas: {stored_gas}")
        
        # Apply IIR filter (α = 0.05)
        second_run_gas = 30_000_000  # Second run uses 30M
        filtered_gas = int(second_run_gas * 0.05 + stored_gas * 0.95)
        print(f"  Second run gas: {second_run_gas} (30M)")
        print(f"  Filtered gas: {filtered_gas} (IIR with α=0.05)")
        
        # Store updated filtered value
        r.setex(gas_history_key, 3600, str(filtered_gas))
        print("  ✓ Gas history workflow complete!")
        
        # Test 5: Pub/Sub
        print("\nTest 5: Pub/Sub test")
        channel = "baseTransactionBroadcast"
        test_tx = '{"signedTx": "0x123..."}'
        
        published = r.publish(channel, test_tx)
        print(f"  Published to '{channel}': {published} subscribers")
        print("  ✓ Pub/Sub works!")
        
        # Cleanup
        print("\nCleaning up test keys...")
        r.delete(key, gas_key, gas_history_key)
        print("✓ Cleanup complete!")
        
        print("\n✅ All Redis tests passed!")
        
    except redis.ConnectionError as e:
        print(f"\n❌ Failed to connect to Redis: {e}")
        print("\nMake sure Redis is running:")
        print("  1. Start Redis: redis-server")
        print("  2. Or use Docker: docker run -d -p 6379:6379 redis:latest")
        return False
    except Exception as e:
        print(f"\n❌ Error: {e}")
        return False
    
    return True

if __name__ == "__main__":
    test_redis()