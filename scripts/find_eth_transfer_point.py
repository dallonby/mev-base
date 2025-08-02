#!/usr/bin/env python3
"""
Find the transaction where ETH transferred to 0xc0ffeefeED8B9d271445cf5D1d24d74D2ca4235E
exceeds the msg.value by scanning backwards through transactions.
"""

import sys
import subprocess
import json
import os
import psycopg2
from datetime import datetime
from typing import Optional, Tuple, Dict
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

TARGET_ADDRESS = "0xc0ffeefeED8B9d271445cf5D1d24d74D2ca4235E"
RPC_URL = "/tmp/op-reth"

def get_db_connection():
    """Create a connection to PostgreSQL database."""
    try:
        conn = psycopg2.connect(
            host=os.getenv('POSTGRES_HOST', 'localhost'),
            port=int(os.getenv('POSTGRES_PORT', '5432')),
            dbname=os.getenv('POSTGRES_DB', 'backrunner_db'),
            user=os.getenv('POSTGRES_USER', 'backrunner'),
            password=os.getenv('POSTGRES_PASSWORD', 'backrunner_password')
        )
        return conn
    except Exception as e:
        print(f"Failed to connect to PostgreSQL: {e}")
        return None

def query_transaction_timestamp(tx_hash: str) -> Optional[Dict[str, any]]:
    """Query when a transaction was first seen in our database."""
    conn = get_db_connection()
    if not conn:
        return None
    
    try:
        with conn.cursor() as cur:
            # Query for the transaction
            cur.execute("""
                SELECT hash, MIN(timestamp) as first_seen, 
                       array_agg(DISTINCT source ORDER BY source) as sources,
                       MIN(block_number) as block_number
                FROM transaction_logs 
                WHERE hash = %s
                GROUP BY hash
            """, (tx_hash,))
            
            result = cur.fetchone()
            if result:
                return {
                    'hash': result[0],
                    'first_seen': result[1],
                    'sources': result[2],
                    'block_number': result[3]
                }
            return None
    except Exception as e:
        print(f"Error querying transaction: {e}")
        return None
    finally:
        conn.close()

def get_tx_info(tx_hash: str) -> Tuple[int, int]:
    """Get block number and transaction index for a given tx hash."""
    cmd = ["cast", "tx", tx_hash, "--rpc-url", RPC_URL, "--json"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise Exception(f"Failed to get tx info: {result.stderr}")
    
    tx_data = json.loads(result.stdout)
    return int(tx_data["blockNumber"], 16), int(tx_data["transactionIndex"], 16)

def get_block_txs(block_number: int) -> list:
    """Get all transaction hashes in a block."""
    cmd = ["cast", "block", str(block_number), "--rpc-url", RPC_URL, "--json"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise Exception(f"Failed to get block: {result.stderr}")
    
    # Handle potential multiple JSON objects in output
    try:
        # Try to parse just the first line which should be the block data
        lines = result.stdout.strip().split('\n')
        block_data = json.loads(lines[0])
        return block_data["transactions"]
    except:
        # Fallback to full parse
        block_data = json.loads(result.stdout)
        return block_data["transactions"]

def trace_call_at_state(block_number: int, tx_index: int, from_addr: str, to_addr: str, 
                       calldata: str, value: int) -> int:
    """
    Use debug_traceCallMany to simulate a call at a specific transaction index within a block.
    Returns the total ETH transferred to the target address.
    """
    # Convert block number and value to hex
    block_hex = hex(block_number)
    value_hex = hex(value)
    
    # Build the transaction object
    tx_obj = {
        "from": from_addr,
        "to": to_addr,
        "input": calldata,  # Note: using 'input' not 'data'
        "value": value_hex,
        "gas": hex(4000000),  # Add gas limit
        "gasPrice": "0x0"
    }
    
    # Build the parameters according to the format you provided
    param1 = json.dumps([{
        "transactions": [tx_obj],
        "blockOverride": {}
    }])
    
    param2 = json.dumps({
        "blockNumber": block_hex,
        "transactionIndex": tx_index  # Use integer, not hex
    })
    
    param3 = json.dumps({
        "stateOverrides": {},
        "tracer": "callTracer"
    })
    
    # Execute debug_traceCallMany
    cmd = ["cast", "rpc", "debug_traceCallMany", param1, param2, param3, "--rpc-url", RPC_URL]
    result = subprocess.run(cmd, capture_output=True, text=True)
    
    if result.returncode != 0:
        print(f"Error executing debug_traceCallMany: {result.stderr}")
        return 0
    
    # Parse the trace to find value transfers to target
    total_transferred = 0
    try:
        # Clean the stdout to handle any extra output
        stdout = result.stdout.strip()
        
        # Sometimes cast returns error messages before/after JSON
        # Try to extract just the JSON part
        if stdout.startswith('[['):
            # Find the end of the JSON array
            bracket_count = 0
            json_end = 0
            for i, char in enumerate(stdout):
                if char == '[':
                    bracket_count += 1
                elif char == ']':
                    bracket_count -= 1
                    if bracket_count == 0:
                        json_end = i + 1
                        break
            
            if json_end > 0:
                stdout = stdout[:json_end]
        
        # debug_traceCallMany returns an array of arrays of results
        results_array = json.loads(stdout)
        
        # Get the first batch, then the first result
        if results_array and len(results_array) > 0 and len(results_array[0]) > 0:
            trace_data = results_array[0][0]
        else:
            # Silently return 0 for empty results
            return 0
        
        # Process only direct calls to the target address, no recursion
        for call in trace_data.get("calls", []):
            if call.get("to", "").lower() == TARGET_ADDRESS.lower():
                value_hex = call.get("value", "0x0")
                if value_hex and value_hex != "0x0":
                    transferred = int(value_hex, 16)
                    total_transferred += transferred
        
    except json.JSONDecodeError as e:
        # Only show error for debugging specific indices
        if block_number == 33671282 and tx_index == 106:
            print(f"JSON parsing error: {e}")
            print(f"Raw output preview: {result.stdout[:200]}...")
    except Exception as e:
        # Only show other errors for debugging  
        if "reverted" not in str(e).lower():
            pass  # Silently ignore most errors
    
    return total_transferred

def get_effective_gas_price(tx_details: dict) -> int:
    """Calculate effective gas price based on transaction type."""
    tx_type = tx_details.get("type", 0)
    
    if isinstance(tx_type, str):
        if tx_type.startswith('0x'):
            tx_type = int(tx_type, 16)
        else:
            tx_type = int(tx_type)
    
    if tx_type < 2:
        # Legacy or EIP-2930 transaction - use gasPrice
        gas_price = tx_details.get("gasPrice", "0x0")
        return int(gas_price, 16) if isinstance(gas_price, str) else gas_price
    else:
        # EIP-1559 transaction - use min(maxFeePerGas, baseFee + maxPriorityFeePerGas)
        # For past transactions, we can use effectiveGasPrice if available
        if "effectiveGasPrice" in tx_details:
            egp = tx_details["effectiveGasPrice"]
            return int(egp, 16) if isinstance(egp, str) else egp
        
        # Otherwise calculate from max fees
        max_fee = tx_details.get("maxFeePerGas", "0x0")
        max_priority = tx_details.get("maxPriorityFeePerGas", "0x0")
        
        max_fee = int(max_fee, 16) if isinstance(max_fee, str) else max_fee
        max_priority = int(max_priority, 16) if isinstance(max_priority, str) else max_priority
        
        # Note: Without baseFee, we can't calculate exact effective price
        # Return maxFeePerGas as upper bound
        return max_fee

def get_tx_details(tx_hash: str) -> dict:
    """Get transaction details from hash."""
    cmd = ["cast", "tx", tx_hash, "--rpc-url", RPC_URL, "--json"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise Exception(f"Failed to get tx details: {result.stderr}")
    
    # Clean the output - sometimes cast includes warnings
    stdout = result.stdout.strip()
    
    # Find the start of the JSON object
    json_start = stdout.find('{')
    if json_start > 0:
        stdout = stdout[json_start:]
    
    # Find the end of the JSON object
    if '{' in stdout:
        bracket_count = 0
        json_end = 0
        for i, char in enumerate(stdout):
            if char == '{':
                bracket_count += 1
            elif char == '}':
                bracket_count -= 1
                if bracket_count == 0:
                    json_end = i + 1
                    break
        
        if json_end > 0:
            stdout = stdout[:json_end]
    
    return json.loads(stdout)

def trace_at_index(block_number: int, tx_index: int, from_addr: str, to_addr: str, 
                   calldata: str, value: int) -> Tuple[int, int]:
    """
    Trace a call at a specific block and transaction index.
    Returns (msg_value, total_transferred)
    """
    total_transferred = trace_call_at_state(
        block_number, tx_index, from_addr, to_addr, calldata, value
    )
    return value, total_transferred

def scan_for_transfer_point(start_tx_hash: str):
    """Scan backwards to find where transfers exceed msg.value."""
    # Get the original transaction details
    tx_details = get_tx_details(start_tx_hash)
    from_addr = tx_details.get("from", "")
    to_addr = tx_details.get("to", "")
    calldata = tx_details.get("input", "0x")
    msg_value = int(tx_details.get("value", "0x0"), 16)
    block_num = int(tx_details.get("blockNumber", "0x0"), 16)
    tx_index = int(tx_details.get("transactionIndex", "0x0"), 16)
    
    # Calculate effective gas price for original transaction
    effective_gas_price = get_effective_gas_price(tx_details)
    
    # Query database for when we first saw this transaction
    db_info = query_transaction_timestamp(start_tx_hash)
    
    # Start scanning backwards from the transaction index
    print(f"\nScanning block {block_num} backwards from index {tx_index}...", end='', flush=True)
    
    found = False
    
    # Scan backwards through transaction indices
    for idx in range(tx_index, -1, -1):
        try:
            # Trace the call at this specific index
            _, transferred = trace_at_index(
                block_num, idx, from_addr, to_addr, calldata, msg_value
            )
            
            if transferred > msg_value and not found:
                found = True
                print(f" found at index {idx}!")
                # Get the transaction hash at this index
                try:
                    block_txs = get_block_txs(block_num)
                    if idx < len(block_txs):
                        tx_hash_at_idx = block_txs[idx]
                        
                        # Get details of this transaction
                        tx_details_at_idx = get_tx_details(tx_hash_at_idx)
                        effective_gas_price_at_idx = get_effective_gas_price(tx_details_at_idx)
                        
                        # Query database for this transaction
                        db_info_found = query_transaction_timestamp(tx_hash_at_idx)
                        
                        # Print distilled results
                        print(f"\n" + "="*80)
                        print(f"ORIGINAL TX: {start_tx_hash}")
                        print(f"WINNER TX:   {tx_hash_at_idx}")
                        print(f"WINNER TO:   {tx_details_at_idx.get('to', 'N/A')}")
                        print(f"-"*80)
                        
                        if db_info:
                            print(f"ORIGINAL TIMESTAMP: {db_info['first_seen']}")
                            print(f"ORIGINAL SOURCE:    {', '.join(db_info['sources'])}")
                        else:
                            print(f"ORIGINAL TIMESTAMP: Not found in database")
                            print(f"ORIGINAL SOURCE:    N/A")
                        
                        if db_info_found:
                            print(f"WINNER TIMESTAMP:   {db_info_found['first_seen']}")
                            print(f"WINNER SOURCE:      {', '.join(db_info_found['sources'])}")
                        else:
                            print(f"WINNER TIMESTAMP:   Not found in database")
                            print(f"WINNER SOURCE:      N/A")
                        
                        if db_info and db_info_found:
                            time_diff = db_info_found['first_seen'] - db_info['first_seen']
                            print(f"TIME DIFFERENCE:    {time_diff.total_seconds():.3f} seconds")
                        
                        print(f"-"*80)
                        print(f"ORIGINAL GAS PRICE: {effective_gas_price:,} wei ({effective_gas_price / 1e9:.4f} gwei)")
                        print(f"WINNER GAS PRICE:   {effective_gas_price_at_idx:,} wei ({effective_gas_price_at_idx / 1e9:.4f} gwei)")
                        print(f"GAS DIFFERENCE:     {effective_gas_price_at_idx - effective_gas_price:,} wei ({(effective_gas_price_at_idx - effective_gas_price) / 1e9:.4f} gwei)")
                        print(f"="*80)
                except Exception as e:
                    print(f"\nError getting transaction at index {idx}: {e}")
                
                # Stop scanning
                return
            
        except Exception as e:
            # Skip indices that fail to trace silently
            continue
    
    if not found:
        print("\n❌ No transaction index found where transfers exceed msg.value")

def trace_specific_call():
    """
    Trace a specific call with hardcoded parameters.
    This can be used for debugging specific scenarios.
    """
    # Parameters as originally requested
    block_number = 33671282
    tx_index = 106  # The index you actually wanted!
    from_addr = "0xc0ffee59f94f54f4f293f01672976408bc1cad7f"
    to_addr = "0xfd5d7d50a11a7bc3b35bb30bb6208d3766ca9532"
    calldata = "0x00000060"
    value = 3600000000  # 0.0000000036 ETH as you originally specified
    
    print(f"Tracing call at block {block_number}, index {tx_index}")
    print(f"From: {from_addr}")
    print(f"To: {to_addr}")
    print(f"Calldata: {calldata}")
    print(f"Value: {value} wei ({value / 1e18} ETH)")
    
    total_transferred = trace_call_at_state(
        block_number, tx_index, from_addr, to_addr, calldata, value
    )
    
    print(f"\nTotal ETH transferred to {TARGET_ADDRESS}: {total_transferred} wei ({total_transferred / 1e18} ETH)")
    if total_transferred > value:
        print(f"✅ Transferred amount exceeds msg.value by {total_transferred - value} wei")
    else:
        print(f"❌ Transferred amount does not exceed msg.value")

def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--trace-specific":
        # Run the specific trace example
        trace_specific_call()
        return
    
    if len(sys.argv) != 2:
        print("Usage: python find_eth_transfer_point.py <transaction_hash>")
        print("   or: python find_eth_transfer_point.py --trace-specific")
        sys.exit(1)
    
    tx_hash = sys.argv[1]
    if not tx_hash.startswith("0x"):
        tx_hash = "0x" + tx_hash
    
    try:
        scan_for_transfer_point(tx_hash)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()