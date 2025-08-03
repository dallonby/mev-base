#!/usr/bin/env python3
"""
Scan Base blockchain for events with a specific topic0.
"""

import sys
import json
import requests
import argparse
from datetime import datetime
import time

# RPC endpoint
RPC_URL = "http://localhost:28545"

def get_current_block():
    """Get the current block number via RPC."""
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    }
    response = requests.post(RPC_URL, json=payload)
    result = response.json()
    return int(result["result"], 16)

def scan_for_events(topic0, start_block, end_block, chunk_size=5000):
    """Scan for events with the specified topic0."""
    all_events = []
    total_chunks = (end_block - start_block + chunk_size - 1) // chunk_size
    
    print(f"Scanning {end_block - start_block + 1} blocks in {total_chunks} chunks...")
    
    for i, chunk_start in enumerate(range(start_block, end_block + 1, chunk_size)):
        chunk_end = min(chunk_start + chunk_size - 1, end_block)
        
        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [{
                "fromBlock": hex(chunk_start),
                "toBlock": hex(chunk_end),
                "topics": [topic0]
            }],
            "id": 1
        }
        
        try:
            response = requests.post(RPC_URL, json=payload, timeout=30)
            result = response.json()
            
            if "result" in result:
                events = result["result"]
                all_events.extend(events)
                print(f"  Chunk {i+1}/{total_chunks}: blocks {chunk_start}-{chunk_end} - found {len(events)} events")
            else:
                print(f"  Chunk {i+1}/{total_chunks}: ERROR - {result.get('error', 'Unknown error')}")
        except Exception as e:
            print(f"  Chunk {i+1}/{total_chunks}: EXCEPTION - {str(e)}")
    
    return all_events

def analyze_events(events, topic0):
    """Analyze the found events."""
    if not events:
        print(f"\n❌ No events found with topic0: {topic0}")
        return
    
    print(f"\n✅ Found {len(events)} events with topic0: {topic0}")
    
    # Group by contract
    contracts = {}
    for event in events:
        addr = event.get('address', '').lower()
        if addr not in contracts:
            contracts[addr] = []
        contracts[addr].append(event)
    
    print(f"\nFound on {len(contracts)} unique contracts:")
    
    # Sort by number of events
    sorted_contracts = sorted(contracts.items(), key=lambda x: len(x[1]), reverse=True)
    
    for addr, contract_events in sorted_contracts[:10]:  # Show top 10
        # Get first and last block for this contract
        blocks = [int(e.get('blockNumber', '0x0'), 16) for e in contract_events]
        min_block = min(blocks)
        max_block = max(blocks)
        
        print(f"\n  Contract: {addr}")
        print(f"    Events: {len(contract_events)}")
        print(f"    First seen: block {min_block}")
        print(f"    Last seen: block {max_block}")
        
        # Show a sample event
        if contract_events:
            sample = contract_events[0]
            print(f"    Sample TX: {sample.get('transactionHash', 'Unknown')}")
            
            # Show additional topics if present
            topics = sample.get('topics', [])
            if len(topics) > 1:
                print(f"    Additional topics: {len(topics) - 1}")
                for i, topic in enumerate(topics[1:4], 1):  # Show up to 3 additional topics
                    print(f"      topic{i}: {topic}")
            
            # Show data preview if present
            data = sample.get('data', '0x')
            if len(data) > 2:
                print(f"    Data: {data[:66]}{'...' if len(data) > 66 else ''}")

def main():
    parser = argparse.ArgumentParser(description='Scan Base blockchain for events with a specific topic0')
    parser.add_argument('topic0', help='The topic0 to search for (hex string starting with 0x)')
    parser.add_argument('-b', '--blocks', type=int, default=43200, 
                        help='Number of blocks to scan backwards from current (default: 43200 ~24h)')
    parser.add_argument('-c', '--chunk-size', type=int, default=5000,
                        help='Number of blocks per chunk (default: 5000)')
    parser.add_argument('-o', '--output', help='Output file for results (JSON format)')
    
    args = parser.parse_args()
    
    # Validate topic0
    if not args.topic0.startswith('0x'):
        print("Error: topic0 must start with '0x'")
        sys.exit(1)
    
    if len(args.topic0) != 66:  # 0x + 64 hex chars
        print(f"Warning: topic0 should be 66 characters (0x + 64 hex), got {len(args.topic0)}")
    
    print(f"Event Scanner for Base")
    print("=" * 60)
    print(f"Topic0: {args.topic0}")
    
    # Get block range
    current_block = get_current_block()
    start_block = current_block - args.blocks
    
    print(f"Current block: {current_block}")
    print(f"Scanning last {args.blocks} blocks (from {start_block})")
    print()
    
    # Scan for events
    start_time = time.time()
    events = scan_for_events(args.topic0, start_block, current_block, args.chunk_size)
    elapsed = time.time() - start_time
    
    print(f"\nScan completed in {elapsed:.2f} seconds")
    
    # Analyze results
    analyze_events(events, args.topic0)
    
    # Save results if requested
    if args.output and events:
        contracts = list(set(e.get('address', '').lower() for e in events))
        output_data = {
            "scan_info": {
                "topic0": args.topic0,
                "start_block": start_block,
                "end_block": current_block,
                "blocks_scanned": current_block - start_block + 1,
                "timestamp": datetime.now().isoformat(),
                "scan_duration_seconds": elapsed
            },
            "summary": {
                "total_events": len(events),
                "unique_contracts": len(contracts)
            },
            "contracts": contracts,
            "events": events[:1000]  # Limit to first 1000 events
        }
        
        with open(args.output, 'w') as f:
            json.dump(output_data, f, indent=2)
        print(f"\nResults saved to {args.output}")

if __name__ == "__main__":
    main()