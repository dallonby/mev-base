#!/usr/bin/env python3
"""
Top up MEV bot wallets to maintain exactly 0.01 ETH balance
"""

import os
import sys
import subprocess
import time
from decimal import Decimal
from typing import List, Tuple
import json

# Load environment variables from .env file
def load_env():
    """Load environment variables from .env file"""
    env_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), '.env')
    if os.path.exists(env_path):
        with open(env_path, 'r') as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith('#') and '=' in line:
                    key, value = line.split('=', 1)
                    os.environ[key.strip()] = value.strip()

load_env()

# Target balance in ETH
TARGET_BALANCE = Decimal("0.01")

# MEV bot wallet addresses
WALLET_ADDRESSES = [
    "0xc0ffEe48945a9518b0B543a2C59dFb102221fBb7",
    "0xc0ffee59F94F54F4F293f01672976408BC1Cad7F",
    "0xC0fFEEA3f806B34888256B0a56DD603a8CFB462b",
    "0xc0fFeE077Edd3997c2a65ef68C71a5BC6400051A",
    "0xC0ffeE722C49A8C105b0d38d32d374EB6EA1321b",
    "0xc0ffeE1D2eDbe6Dad64AF3D05C0421cBECdCb83a",
    "0xc0FFeEB180b16D93f8E9713Ac7a5D29f43338643",
    "0xC0ffEE445A22e6228cFb77Ec0483C426Fc856161",
    "0xC0FFeE2E2E109A7321Aa17F364A5425E48E1b219",
    "0xc0FFEE666bDd7F9897C72F9195955a16D63C0ac4",
    "0xc0ffEe17d520056942531cB6D4D6251Ff8163Bb1",
    "0xC0Ffee8FD2291aB011f1B6786E8b2BC9e13977c3",
    "0xC0FfeE2D423e04E42a02018b71455753878d9De2",
    "0xc0ffeE1864731c3c33a1967fc8E0fbF454a6a006",
    "0xc0fFEe2a32BC8d7799764EF72CAA075276908484",
    "0xc0ffEE48a9e45Ad104a5B38c0844aACC5B82e6f8",
    "0xc0FFEe90C2CfA398E0aF37EEADb39D200E331EEC",
    "0xC0ffeeAf9AADcF05e268383dd2c96466B0CccA59",
    "0xC0FfeEd427cBB2d1212fa92Ff2cAD061ecC1F3b0",
    "0xC0fFEE2F93B3f53B081F9E775dC47cf54587237e",
    "0xC0fFEE62570f490c781B24e3C3E29B5f94C726a3"
]

# ANSI color codes
GREEN = '\033[92m'
YELLOW = '\033[93m'
RED = '\033[91m'
RESET = '\033[0m'
BOLD = '\033[1m'


def run_cast_command(args: List[str]) -> Tuple[bool, str]:
    """Run a cast command and return success status and output"""
    try:
        result = subprocess.run(
            ["cast"] + args,
            capture_output=True,
            text=True,
            check=True,
            timeout=10  # 10 second timeout
        )
        return True, result.stdout.strip()
    except subprocess.TimeoutExpired:
        return False, "Command timed out"
    except subprocess.CalledProcessError as e:
        return False, e.stderr.strip()


def get_balance(address: str) -> Decimal:
    """Get the balance of an address in ETH"""
    success, output = run_cast_command([
        "balance",
        address,
        "--rpc-url", "https://mainnet.base.org"
    ])
    
    if not success:
        print(f"{RED}Failed to get balance for {address}: {output}{RESET}")
        return Decimal("0")
    
    # Extract the first line which contains the balance
    balance_line = output.split('\n')[0].strip()
    
    try:
        # Convert from wei to ETH
        wei_balance = int(balance_line)
        eth_balance = Decimal(wei_balance) / Decimal(10**18)
        return eth_balance
    except ValueError:
        print(f"{RED}Failed to parse balance for {address}: {balance_line}{RESET}")
        return Decimal("0")


def get_nonce(address: str) -> int:
    """Get the current nonce for an address"""
    success, output = run_cast_command([
        "nonce",
        address,
        "--rpc-url", "https://mainnet.base.org"
    ])
    
    if success:
        try:
            return int(output.split('\n')[0].strip())
        except ValueError:
            return 0
    return 0


def send_eth(from_key: str, to_address: str, amount_eth: Decimal, nonce: int = None) -> bool:
    """Send ETH from master account to target address"""
    # Convert ETH to wei
    amount_wei = int(amount_eth * Decimal(10**18))
    
    print(f"  Sending {amount_eth:.6f} ETH to {to_address}...")
    
    cmd = [
        "send",
        to_address,
        "--value", str(amount_wei),
        "--private-key", from_key,
        "--rpc-url", "https://mainnet.base.org",
        "--priority-gas-price", "5000000",  # 0.005 gwei
        "--gas-limit", "21000",
        "--async"  # Don't wait for transaction to be mined
    ]
    
    # Add nonce if specified
    if nonce is not None:
        cmd.extend(["--nonce", str(nonce)])
    
    success, output = run_cast_command(cmd)
    
    if success:
        # Extract transaction hash from output
        # With --async, cast returns just the transaction hash
        tx_hash = output.strip()
        if tx_hash.startswith("0x") and len(tx_hash) == 66:
            print(f"  {GREEN}✓ Transaction broadcast: {tx_hash}{RESET}")
            return True
        else:
            print(f"  {GREEN}✓ Transaction sent{RESET}")
            return True
    else:
        print(f"  {RED}✗ Failed to send transaction: {output}{RESET}")
        return False


def main():
    """Main function to top up all wallets"""
    # Check for command line arguments
    auto_yes = '--yes' in sys.argv or '-y' in sys.argv
    dry_run = '--dry-run' in sys.argv or '-n' in sys.argv
    
    # Get master account private key from environment
    master_key = os.getenv("MASTER_WALLET_KEY")
    if not master_key:
        print(f"{RED}Error: MASTER_WALLET_KEY not found in environment or .env file{RESET}")
        print("Please set MASTER_WALLET_KEY in your .env file")
        sys.exit(1)
    
    # Get master account address
    success, master_address = run_cast_command(["wallet", "address", master_key])
    if not success:
        print(f"{RED}Error: Failed to get master wallet address{RESET}")
        sys.exit(1)
    
    print(f"{BOLD}MEV Wallet Top-up Script{RESET}")
    print(f"Master account: {master_address}")
    
    # Get master balance
    master_balance = get_balance(master_address)
    print(f"Master balance: {master_balance:.6f} ETH\n")
    
    # Track totals
    total_needed = Decimal("0")
    wallets_needing_topup = []
    
    # Check each wallet
    print(f"{BOLD}Checking wallet balances...{RESET}")
    for i, address in enumerate(WALLET_ADDRESSES):
        balance = get_balance(address)
        needed = TARGET_BALANCE - balance
        
        # Color code the output
        if balance == TARGET_BALANCE:
            status = f"{GREEN}✓{RESET}"
            color = GREEN
        elif balance < TARGET_BALANCE:
            status = f"{YELLOW}↑{RESET}"
            color = YELLOW
        else:
            status = f"{RED}↓{RESET}"
            color = RED
        
        print(f"{status} Wallet {i:2d}: {address} - Balance: {color}{balance:.6f}{RESET} ETH", end="")
        
        if needed > 0:
            print(f" (needs {needed:.6f} ETH)")
            total_needed += needed
            wallets_needing_topup.append((address, needed))
        elif needed < 0:
            print(f" (excess {-needed:.6f} ETH)")
        else:
            print(" (exact)")
    
    print(f"\n{BOLD}Summary:{RESET}")
    print(f"Wallets needing top-up: {len(wallets_needing_topup)}")
    print(f"Total ETH needed: {total_needed:.6f} ETH")
    
    # Check if master has enough balance
    if total_needed > master_balance:
        print(f"\n{RED}Error: Insufficient master balance!{RESET}")
        print(f"Need {total_needed:.6f} ETH but only have {master_balance:.6f} ETH")
        sys.exit(1)
    
    if len(wallets_needing_topup) == 0:
        print(f"\n{GREEN}All wallets are at target balance!{RESET}")
        return
    
    # Confirm before proceeding
    print(f"\n{YELLOW}Ready to send {total_needed:.6f} ETH to {len(wallets_needing_topup)} wallets.{RESET}")
    
    if dry_run:
        print(f"\n{YELLOW}DRY RUN MODE - No transactions will be sent{RESET}")
        return
    
    if not auto_yes:
        response = input("Proceed? (y/N): ")
        if response.lower() != 'y':
            print("Aborted.")
            return
    else:
        print("Auto-confirming (--yes flag)")
    
    # Send transactions
    print(f"\n{BOLD}Sending transactions...{RESET}")
    successful = 0
    failed = 0
    
    # Get starting nonce
    current_nonce = get_nonce(master_address)
    print(f"Starting nonce: {current_nonce}")
    
    for i, (address, amount) in enumerate(wallets_needing_topup):
        # Use incrementing nonce for each transaction
        if send_eth(master_key, address, amount, nonce=current_nonce + i):
            successful += 1
        else:
            failed += 1
        
        # Small delay between transactions
        time.sleep(0.1)  # Reduced delay since we're using explicit nonces
    
    # Final summary
    print(f"\n{BOLD}Complete!{RESET}")
    print(f"{GREEN}Successful: {successful}{RESET}")
    if failed > 0:
        print(f"{RED}Failed: {failed}{RESET}")
    
    # Show final master balance
    final_balance = get_balance(master_address)
    print(f"\nMaster balance after top-up: {final_balance:.6f} ETH")
    print(f"ETH spent: {master_balance - final_balance:.6f} ETH")


if __name__ == "__main__":
    # Check for help flag
    if '--help' in sys.argv or '-h' in sys.argv:
        print("MEV Wallet Top-up Script")
        print("\nUsage: python3 topup_wallets.py [OPTIONS]")
        print("\nOptions:")
        print("  -y, --yes      Automatically confirm transactions (non-interactive)")
        print("  -n, --dry-run  Show what would be done without sending transactions")
        print("  -h, --help     Show this help message")
        sys.exit(0)
    
    main()