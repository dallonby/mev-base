#!/usr/bin/env python3
"""Test PostgreSQL query functionality."""

import os
import sys
from dotenv import load_dotenv

# Add the current directory to Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# Load environment variables
load_dotenv()

# Import our query function
from find_eth_transfer_point import query_transaction_timestamp

# Test with a sample transaction hash
test_hash = "0x7c7d0f65508f0cf4f678e8d93da2dc0b7f8c5e8f7b6a5d4c3b2a1908f7e6d5c4"  # Example hash

print(f"Testing database query for transaction: {test_hash}")
result = query_transaction_timestamp(test_hash)

if result:
    print(f"\nTransaction found in database:")
    print(f"  Hash: {result['hash']}")
    print(f"  First seen: {result['first_seen']}")
    print(f"  Sources: {', '.join(result['sources'])}")
    print(f"  Block number: {result['block_number']}")
else:
    print("\nTransaction not found in database")