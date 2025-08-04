#!/usr/bin/env python3
"""
Monitor MEV results and analyze failed transactions.
Watches mev_results.jsonl for new entries and checks if transactions succeeded.
If they failed, analyzes who beat us to the opportunity.
"""

import json
import time
import subprocess
import sys
import os
from datetime import datetime, timezone
from typing import Dict, Optional, List, Tuple
import argparse
import psycopg2
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

# Constants
MEV_RESULTS_FILE = "/home/ubuntu/Source/reth-mev-standalone/mev_results.jsonl"
MEV_ANALYSIS_LOG = "/home/ubuntu/Source/reth-mev-standalone/mev_analysis.log"
RPC_URL = "/tmp/op-reth"
COFFEE_ADDRESS = "0xc0ffeefeED8B9d271445cf5D1d24d74D2ca4235E"

def log_output(message: str, log_file: str = MEV_ANALYSIS_LOG):
    """Print to console and append to log file."""
    print(message)
    try:
        with open(log_file, 'a') as f:
            f.write(message + '\n')
    except Exception as e:
        print(f"Warning: Could not write to log file: {e}")

def get_flashblock_info(tx_hash: str) -> Optional[int]:
    """Query PostgreSQL to get the actual flashblock for a transaction."""
    try:
        conn = psycopg2.connect(
            host=os.getenv('POSTGRES_HOST', 'localhost'),
            port=int(os.getenv('POSTGRES_PORT', '5432')),
            dbname=os.getenv('POSTGRES_DB', 'backrunner_db'),
            user=os.getenv('POSTGRES_USER', 'backrunner'),
            password=os.getenv('POSTGRES_PASSWORD', 'backrunner_password')
        )
        
        with conn.cursor() as cur:
            # Query for the flashblock info
            query = "SELECT DISTINCT SUBSTRING(source FROM 'flashblock_([0-9]+)')::INTEGER as flashblock FROM transaction_logs WHERE hash = %s AND source LIKE %s LIMIT 1"
            cur.execute(query, (tx_hash, 'flashblock_%'))
            
            result = cur.fetchone()
            if result and result[0] is not None:
                return result[0]
                
        conn.close()
        return None
        
    except Exception as e:
        # Log error if debug mode
        if os.getenv('DEBUG_FLASHBLOCK'):
            import traceback
            print(f"  [DEBUG] DB error: {e}")
            traceback.print_exc()
        return None

def get_transaction_receipt(tx_hash: str) -> Optional[Dict]:
    """Get transaction receipt using cast."""
    try:
        result = subprocess.run(
            ["cast", "receipt", tx_hash, "--json", "--rpc-url", RPC_URL],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
        return None
    except (subprocess.TimeoutExpired, json.JSONDecodeError):
        return None

def get_block_info(block_number: int) -> Optional[Dict]:
    """Get block information using cast."""
    try:
        result = subprocess.run(
            ["cast", "block", str(block_number), "--json", "--rpc-url", RPC_URL],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0:
            return json.loads(result.stdout)
        return None
    except (subprocess.TimeoutExpired, json.JSONDecodeError):
        return None

def find_competitor_in_block(block_number: int, flashblock_index: int, our_tx_hash: str) -> Optional[Dict]:
    """Find the transaction that beat us in the same block."""
    block_info = get_block_info(block_number)
    if not block_info or 'transactions' not in block_info:
        return None
    
    # Look for transactions to coffee address in the same flashblock
    for tx_hash in block_info['transactions']:
        if tx_hash.lower() == our_tx_hash.lower():
            continue  # Skip our own transaction
            
        # Get transaction details
        try:
            result = subprocess.run(
                ["cast", "tx", tx_hash, "--json", "--rpc-url", RPC_URL],
                capture_output=True,
                text=True,
                timeout=10
            )
            if result.returncode == 0:
                tx = json.loads(result.stdout)
                # Check if it's to the coffee address
                if tx.get('to', '').lower() == COFFEE_ADDRESS.lower():
                    # Get the flashblock index (transaction position in block)
                    tx_index = block_info['transactions'].index(tx_hash)
                    tx['transactionIndex'] = tx_index
                    tx['flashblockIndex'] = tx_index // 20  # Assuming 20 txs per flashblock
                    
                    # Calculate effective gas price
                    if 'effectiveGasPrice' in tx:
                        gas_price_gwei = int(tx['effectiveGasPrice'], 16) / 1e9
                        tx['gasPriceGwei'] = gas_price_gwei
                    
                    return tx
        except (subprocess.TimeoutExpired, json.JSONDecodeError, ValueError):
            continue
    
    return None

def run_find_eth_transfer(tx_hash: str) -> Optional[Dict]:
    """Run the find_eth_transfer_point.py script and parse its output."""
    script_path = "/home/ubuntu/Source/reth-mev-standalone/scripts/run_find_eth_transfer.sh"
    
    try:
        # Run the script with transaction hash
        cmd = [script_path, tx_hash]
        
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=60  # Give it 60 seconds to complete
        )
        
        if result.returncode != 0:
            if "ModuleNotFoundError" in result.stderr:
                print(f"  ⚠️  find_eth_transfer requires psycopg2. Install with: pip install psycopg2-binary")
            else:
                print(f"Error running find_eth_transfer: {result.stderr}")
            return None
        
        # Parse the output to extract key information
        output_lines = result.stdout.strip().split('\n')
        winner_info = {}
        
        for line in output_lines:
            if "Found reversion state change at index" in line or "Found opportunity at index" in line:
                # Extract the winning transaction index
                try:
                    tx_index = int(line.split("index")[1].split("!")[0].strip())
                    winner_info['tx_index'] = tx_index
                    winner_info['flashblock_index'] = tx_index // 20
                except (IndexError, ValueError):
                    pass
            elif "WINNER TX:" in line and "0x" in line:
                # Extract winner transaction hash
                parts = line.split()
                for part in parts:
                    if part.startswith('0x') and len(part) == 66:
                        winner_info['tx_hash'] = part
                        break
            elif "WINNER GAS PRICE:" in line and "gwei" in line:
                # Extract gas price
                try:
                    # Line format: "WINNER GAS PRICE:   8,155,775 wei (0.0082 gwei)"
                    gwei_part = line.split("(")[1].split("gwei")[0].strip()
                    gas_price = float(gwei_part)
                    winner_info['gas_price_gwei'] = gas_price
                except (IndexError, ValueError):
                    pass
            elif "WINNER TO:" in line and "0x" in line:
                # Extract sender address (winner TO is the bot address)
                parts = line.split()
                for part in parts:
                    if part.startswith('0x') and len(part) == 42:
                        winner_info['sender'] = part
                        break
        
        return winner_info if winner_info else None
        
    except subprocess.TimeoutExpired:
        print(f"Timeout running find_eth_transfer for block {block_number}")
        return None

def analyze_failed_transaction(mev_result: Dict) -> Dict:
    """Analyze why our MEV transaction failed."""
    analysis = {
        'our_tx': mev_result['transaction_hash'],
        'block': mev_result['block_number'],
        'flashblock': mev_result['flashblock_index'],
        'strategy': mev_result['strategy'],
        'expected_profit_eth': mev_result['expected_profit_eth'],
        'status': 'unknown',
        'competitor': None
    }
    
    # Check if our transaction made it on-chain
    receipt = get_transaction_receipt(mev_result['transaction_hash'])
    
    if receipt:
        # Transaction was included
        if receipt.get('status') == '0x1':
            analysis['status'] = 'success'
            analysis['gas_used'] = int(receipt.get('gasUsed', '0x0'), 16)
            analysis['effective_gas_price_gwei'] = int(receipt.get('effectiveGasPrice', '0x0'), 16) / 1e9
        else:
            analysis['status'] = 'reverted'
            analysis['revert_reason'] = 'Transaction reverted on-chain (likely frontrun)'
            analysis['gas_used'] = int(receipt.get('gasUsed', '0x0'), 16)
            analysis['effective_gas_price_gwei'] = int(receipt.get('effectiveGasPrice', '0x0'), 16) / 1e9
    else:
        # Transaction not found on-chain
        analysis['status'] = 'not_included'
    
    # For both reverted and not_included, find who beat us
    if analysis['status'] in ['reverted', 'not_included']:
        print(f"  Running find_eth_transfer analysis for tx {mev_result['transaction_hash']}...")
        
        # Run find_eth_transfer with our transaction hash to see who beat us
        winner = run_find_eth_transfer(mev_result['transaction_hash'])
        
        if winner:
            analysis['competitor'] = winner
            analysis['beaten_by'] = winner.get('tx_hash', 'unknown')
            analysis['competitor_gas_price_gwei'] = winner.get('gas_price_gwei', 0)
            
            # Query actual flashblock info from PostgreSQL
            our_flashblock = get_flashblock_info(mev_result['transaction_hash'])
            winner_flashblock = get_flashblock_info(winner.get('tx_hash', ''))
            
            # Debug output
            if os.getenv('DEBUG_FLASHBLOCK'):
                print(f"  [DEBUG] DB query results - Our: {our_flashblock}, Winner: {winner_flashblock}")
            
            # Use DB flashblock info if available, otherwise fall back to MEV result
            if our_flashblock is None:
                our_flashblock = mev_result.get('flashblock_index')
            
            if winner_flashblock is not None and our_flashblock is not None:
                analysis['same_flashblock'] = winner_flashblock == our_flashblock
                analysis['our_flashblock'] = our_flashblock
                analysis['winner_flashblock'] = winner_flashblock
                # Debug output
                if os.getenv('DEBUG_FLASHBLOCK'):
                    print(f"  [DEBUG] Our flashblock: {our_flashblock}, Winner flashblock: {winner_flashblock}")
            else:
                # Fallback to index-based calculation if DB not available
                if winner.get('tx_index') is not None:
                    winner_flashblock = winner['tx_index'] // 20
                    analysis['same_flashblock'] = winner_flashblock == mev_result['flashblock_index']
                else:
                    analysis['same_flashblock'] = False
            
            # Get more details about competitor transaction
            if winner.get('tx_hash'):
                competitor_receipt = get_transaction_receipt(winner['tx_hash'])
                if competitor_receipt:
                    analysis['competitor']['gas_used'] = int(competitor_receipt.get('gasUsed', '0x0'), 16)
    
    return analysis

def format_analysis(analysis: Dict) -> str:
    """Format analysis results for display."""
    lines = []
    lines.append(f"\n{'='*80}")
    lines.append(f"MEV Transaction Analysis - Block {analysis['block']} Flashblock {analysis['flashblock']}")
    lines.append(f"{'='*80}")
    lines.append(f"Strategy: {analysis['strategy']}")
    lines.append(f"Expected Profit: {analysis['expected_profit_eth']:.6f} ETH")
    lines.append(f"Our TX: {analysis['our_tx']}")
    lines.append(f"Status: {analysis['status'].upper()}")
    
    if analysis['status'] == 'success':
        lines.append(f"✅ Transaction succeeded!")
        lines.append(f"   Gas Used: {analysis.get('gas_used', 'N/A'):,}")
        lines.append(f"   Gas Price: {analysis.get('effective_gas_price_gwei', 0):.4f} gwei")
    elif analysis['status'] == 'reverted':
        lines.append(f"❌ Transaction reverted on-chain")
        lines.append(f"   Reason: {analysis.get('revert_reason', 'Unknown')}")
        lines.append(f"   Gas Used: {analysis.get('gas_used', 'N/A'):,}")
        lines.append(f"   Our Gas Price: {analysis.get('effective_gas_price_gwei', 0):.4f} gwei")
    elif analysis['status'] == 'not_included':
        lines.append(f"❌ Transaction not included in block")
    
    # Show competitor info for both reverted and not_included
    if analysis['status'] in ['reverted', 'not_included'] and analysis.get('competitor'):
        comp = analysis['competitor']
        lines.append(f"\n🏁 BEATEN BY:")
        lines.append(f"   TX Hash: {comp.get('tx_hash', 'unknown')}")
        lines.append(f"   Sender: {comp.get('sender', 'unknown')}")
        lines.append(f"   Gas Price: {comp.get('gas_price_gwei', 0):.4f} gwei")
        lines.append(f"   TX Index: {comp.get('tx_index', 'unknown')}")
        lines.append(f"   Same Flashblock: {'YES' if analysis.get('same_flashblock') else 'NO'}")
        
        # Always show flashblock numbers if available
        if 'our_flashblock' in analysis and 'winner_flashblock' in analysis:
            lines.append(f"   Flashblocks: Us=flashblock_{analysis['our_flashblock']}, Winner=flashblock_{analysis['winner_flashblock']}")
        elif 'our_flashblock' in analysis:
            lines.append(f"   Our Flashblock: flashblock_{analysis['our_flashblock']}")
        
        if analysis.get('same_flashblock'):
            lines.append(f"   ⚡ Lost in same flashblock! Need higher gas price.")
        else:
            if 'our_flashblock' in analysis and 'winner_flashblock' in analysis:
                lines.append(f"   📍 Lost to earlier flashblock ({analysis['winner_flashblock']} < {analysis['our_flashblock']}).")
            else:
                lines.append(f"   📍 Lost to earlier flashblock.")
    elif analysis['status'] in ['reverted', 'not_included'] and not analysis.get('competitor'):
        lines.append(f"\n❓ Could not determine who beat us")
    
    lines.append(f"{'='*80}\n")
    return '\n'.join(lines)

def tail_file(filename: str, last_position: int = 0) -> Tuple[List[str], int]:
    """Read new lines from file starting from last position."""
    new_lines = []
    try:
        with open(filename, 'r') as f:
            f.seek(last_position)
            new_lines = f.readlines()
            last_position = f.tell()
    except FileNotFoundError:
        pass
    return new_lines, last_position

def main():
    parser = argparse.ArgumentParser(description='Monitor MEV results and analyze failures')
    parser.add_argument('--once', action='store_true', help='Run once and exit')
    parser.add_argument('--last', type=int, default=0, help='Analyze last N entries')
    parser.add_argument('--interval', type=int, default=5, help='Check interval in seconds')
    parser.add_argument('--log', default=MEV_ANALYSIS_LOG, help='Log file path')
    args = parser.parse_args()
    
    # Add timestamp to log when starting
    start_time = datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')
    log_output(f"\n{'='*80}", args.log)
    log_output(f"MEV Results Monitor Started at {start_time}", args.log)
    log_output(f"{'='*80}", args.log)
    
    log_output(f"🔍 MEV Results Monitor Started", args.log)
    log_output(f"📊 Watching: {MEV_RESULTS_FILE}", args.log)
    log_output(f"🌐 RPC: {RPC_URL}", args.log)
    log_output(f"📝 Logging to: {args.log}", args.log)
    
    if args.last > 0:
        # Analyze last N entries
        try:
            with open(MEV_RESULTS_FILE, 'r') as f:
                lines = f.readlines()
                start_idx = max(0, len(lines) - args.last)
                for line in lines[start_idx:]:
                    try:
                        result = json.loads(line.strip())
                        log_output(f"\nAnalyzing: {result['transaction_hash']}", args.log)
                        analysis = analyze_failed_transaction(result)
                        log_output(format_analysis(analysis), args.log)
                    except json.JSONDecodeError:
                        continue
        except FileNotFoundError:
            log_output(f"File not found: {MEV_RESULTS_FILE}", args.log)
        return
    
    # Monitor mode
    last_position = 0
    if not args.once:
        # Start from end of file for continuous monitoring
        try:
            with open(MEV_RESULTS_FILE, 'r') as f:
                f.seek(0, 2)  # Go to end of file
                last_position = f.tell()
        except FileNotFoundError:
            pass
    
    log_output(f"\n⏰ Checking every {args.interval} seconds...", args.log)
    
    while True:
        new_lines, last_position = tail_file(MEV_RESULTS_FILE, last_position)
        
        for line in new_lines:
            try:
                result = json.loads(line.strip())
                log_output(f"\n🆕 New MEV result detected: {result['transaction_hash']}", args.log)
                
                # Parse timestamp and calculate age
                timestamp_str = result.get('timestamp', '')
                if timestamp_str:
                    # Parse timestamp like "2025-08-04 15:40:59.698 UTC"
                    timestamp = datetime.strptime(timestamp_str.replace(' UTC', ''), '%Y-%m-%d %H:%M:%S.%f')
                    timestamp = timestamp.replace(tzinfo=timezone.utc)
                    age_seconds = (datetime.now(timezone.utc) - timestamp).total_seconds()
                    
                    # Wait if the result is too fresh
                    if age_seconds < 3:
                        wait_time = 3 - age_seconds
                        log_output(f"  ⏳ Waiting {wait_time:.1f}s for block finalization...", args.log)
                        time.sleep(wait_time)
                else:
                    # No timestamp, wait default time
                    time.sleep(2)
                
                # Analyze the transaction
                analysis = analyze_failed_transaction(result)
                log_output(format_analysis(analysis), args.log)
                
            except (json.JSONDecodeError, ValueError) as e:
                continue
        
        if args.once:
            break
            
        time.sleep(args.interval)

if __name__ == "__main__":
    main()