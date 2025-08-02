#!/usr/bin/env python3
"""
Simulate a transaction at a specific block and transaction index.
This simulates the transaction with the state as it would be after the specified index.
"""

import sys
import subprocess
import json

RPC_URL = "/tmp/op-reth"

def simulate_tx_at_index(tx_hash: str, block_number: int, tx_index: int):
    """Simulate a transaction at a specific block and transaction index."""
    
    # Get transaction details
    cmd = ["cast", "tx", tx_hash, "--rpc-url", RPC_URL, "--json"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise Exception(f"Failed to get tx info: {result.stderr}")
    
    tx_data = json.loads(result.stdout)
    
    # Create the transaction object for tracing
    tx_obj = {
        "from": tx_data["from"],
        "to": tx_data["to"],
        "value": tx_data.get("value", "0x0"),
        "input": tx_data["input"],
        "gas": tx_data.get("gasLimit", "0x1000000")
    }
    
    # We need to trace at the block AFTER all transactions up to tx_index have been executed
    # This is a bit tricky - we need to use debug_traceCall with the block state
    
    # For now, let's use a simpler approach - trace the call at the target block
    print(f"Simulating transaction {tx_hash}")
    print(f"At block {block_number} (after transaction index {tx_index})")
    print(f"From: {tx_obj['from']}")
    print(f"To: {tx_obj['to']}")
    print(f"Value: {int(tx_obj['value'], 16)} wei ({int(tx_obj['value'], 16) / 1e18:.9f} ETH)")
    print(f"Input: {tx_obj['input']}")
    
    # Use debug_traceCall to simulate the transaction
    block_hex = hex(block_number)
    
    # Build the parameters for debug_traceCall
    params = [
        tx_obj,
        block_hex,
        {"tracer": "callTracer"}
    ]
    
    # Execute debug_traceCall
    cmd = ["cast", "rpc", "debug_traceCall", json.dumps(params[0]), json.dumps(params[1]), json.dumps(params[2]), "--rpc-url", RPC_URL]
    result = subprocess.run(cmd, capture_output=True, text=True)
    
    if result.returncode != 0:
        print(f"Error: {result.stderr}")
        return
    
    # Parse the trace
    try:
        trace_data = json.loads(result.stdout)
        
        # Find transfers to 0xc0ffeefeED8B9d271445cf5D1d24d74D2ca4235E
        target = "0xc0ffeefeED8B9d271445cf5D1d24d74D2ca4235E"
        total_transferred = 0
        
        def find_transfers(call_data, depth=0):
            nonlocal total_transferred
            
            # Check if this call transfers value to our target
            if call_data.get("to", "").lower() == target.lower():
                value_hex = call_data.get("value", "0x0")
                if value_hex and value_hex != "0x0":
                    value = int(value_hex, 16)
                    total_transferred += value
                    print(f"  {'  ' * depth}Transfer to {target}: {value} wei")
            
            # Recursively check subcalls
            for subcall in call_data.get("calls", []):
                find_transfers(subcall, depth + 1)
        
        print("\nTrace results:")
        find_transfers(trace_data)
        
        print(f"\nTotal transferred to {target}: {total_transferred} wei ({total_transferred / 1e18:.9f} ETH)")
        msg_value = int(tx_obj['value'], 16)
        if total_transferred > msg_value:
            print(f"✅ Transferred amount EXCEEDS msg.value by {total_transferred - msg_value} wei")
        else:
            print(f"❌ Transferred amount does not exceed msg.value")
            
    except Exception as e:
        print(f"Error parsing trace: {e}")
        print(f"Raw output: {result.stdout}")

def main():
    if len(sys.argv) != 4:
        print("Usage: python simulate_at_index.py <transaction_hash> <block_number> <tx_index>")
        sys.exit(1)
    
    tx_hash = sys.argv[1]
    if not tx_hash.startswith("0x"):
        tx_hash = "0x" + tx_hash
        
    block_number = int(sys.argv[2])
    tx_index = int(sys.argv[3])
    
    try:
        simulate_tx_at_index(tx_hash, block_number, tx_index)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()