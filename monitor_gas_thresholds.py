#!/usr/bin/env python3
"""
Monitor Gas History Thresholds in Redis
Shows current gas consumption patterns and bound adjustments for MEV optimization
"""

import os
import redis
import time
from datetime import datetime
import sys

# ANSI color codes
GREEN = '\033[92m'
YELLOW = '\033[93m'
RED = '\033[91m'
BLUE = '\033[94m'
RESET = '\033[0m'
BOLD = '\033[1m'

def format_gas(gas_value):
    """Format gas value with M suffix"""
    if gas_value is None:
        return "None"
    gas_m = gas_value / 1_000_000
    return f"{gas_m:.1f}M"

def get_gas_color(gas_value):
    """Get color based on gas usage"""
    if gas_value is None:
        return RESET
    gas_m = gas_value / 1_000_000
    if gas_m <= 35:
        return GREEN
    elif gas_m <= 50:
        return YELLOW
    else:
        return RED

def calculate_bounds_adjustment(gas_value, initial_qty=1_000_000):
    """Calculate how bounds would be adjusted based on gas usage"""
    if gas_value is None:
        return None, None, "No data", 1.0
    
    target_gas = 35_000_000
    
    # This matches the actual adjustment logic in gradient_descent_binary.rs
    if gas_value > target_gas * 2:
        adjustment = 0.5  # Reduce to 50% if way too much gas
        status = f"⚠ Very high gas ({gas_value/target_gas:.1f}x target)"
    elif gas_value > target_gas:
        adjustment = 0.8  # Reduce to 80% if slightly over
        status = f"⚠ High gas ({gas_value/target_gas:.1f}x target)"
    elif gas_value < target_gas / 2:
        adjustment = 1.5  # Increase to 150% if plenty of headroom
        status = "✓ Very efficient"
    else:
        adjustment = 1.0  # Keep as is
        status = "✓ Efficient"
    
    # Note: The actual bounds depend on the config's default_value
    # Typically: lower = default_value/5, upper = default_value*1000
    # After adjustment: upper *= adjustment factor
    return None, None, status, adjustment

def get_contract_name(address):
    """Get friendly name for known contract addresses"""
    known_contracts = {
        "0x940181a94A35A4569E4529A3CDfB74e38FD98631": "Aerodrome Router",
        "0xd0b53D9277642d899DF5C87A3966A349A798F224": "Uniswap V3 Router", 
        "0x198EF79F1F515F02dFE9e3115eD9fC07183f02fC": "BaseSwap Router",
        "0x2626664c2603336E57B271c5C0b26F421741e481": "Uniswap V3 Router 2",
        "0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43": "Aerodrome Router 2",
    }
    return known_contracts.get(address, "Unknown Contract")

def monitor_gas_thresholds(continuous=False):
    """Monitor gas thresholds stored in Redis"""
    
    # Get Redis connection details
    redis_host = os.environ.get('REDIS_LOCAL_HOST', 'localhost')
    redis_port = int(os.environ.get('REDIS_LOCAL_PORT', '6379'))
    redis_password = os.environ.get('REDIS_LOCAL_PASSWORD', '9PmITfN1LiGZDYNr4Gp0LxU9Dq')
    
    try:
        r = redis.Redis(
            host=redis_host,
            port=redis_port,
            password=redis_password,
            decode_responses=True
        )
        
        # Test connection
        r.ping()
        
        while True:
            # Clear screen for continuous mode
            if continuous:
                os.system('clear' if os.name == 'posix' else 'cls')
            
            print(f"{BOLD}MEV Gas History Monitor{RESET}")
            print(f"Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
            print("=" * 100)
            print(f"\n{BOLD}Target Gas:{RESET} {GREEN}35M{RESET} | {BOLD}IIR Filter:{RESET} α=0.05 | {BOLD}TTL:{RESET} 24 hours")
            print("=" * 100)
            
            # Get all gas history keys
            keys = list(r.scan_iter(match="mev:gas:*"))
            
            if not keys:
                print(f"\n{YELLOW}No gas history data found in Redis{RESET}")
                print("\nGas history will be populated after the first optimization run.")
            else:
                print(f"\n{BOLD}{'Address':<44} {'Contract':<20} {'Gas Used':<12} {'TTL':<8} {'Multiplier':<12} {'Next Adjust':<12} {'Status':<30}{RESET}")
                print("-" * 120)
                
                # Sort keys for consistent display
                keys.sort()
                
                total_gas = 0
                count = 0
                
                for key in keys:
                    # Extract address from key
                    address = key.replace("mev:gas:", "")
                    
                    # Get gas value and TTL
                    gas_str = r.get(key)
                    ttl = r.ttl(key)
                    
                    if gas_str:
                        try:
                            # Try to parse as JSON first
                            actual_multiplier = None
                            if gas_str.startswith('{'):
                                try:
                                    import json
                                    data = json.loads(gas_str)
                                    gas_value = int(data.get('gas', 0))
                                    actual_multiplier = data.get('multiplier')
                                except:
                                    gas_value = int(gas_str)
                            else:
                                gas_value = int(gas_str)
                            
                            total_gas += gas_value
                            count += 1
                            
                            # Get contract name
                            contract_name = get_contract_name(address)
                            
                            # Calculate bounds adjustment
                            lower, upper, status, adjustment = calculate_bounds_adjustment(gas_value)
                            
                            # Format output with color
                            gas_color = get_gas_color(gas_value)
                            gas_formatted = format_gas(gas_value)
                            
                            # Format TTL
                            if ttl > 0:
                                ttl_min = ttl // 60
                                ttl_sec = ttl % 60
                                ttl_str = f"{ttl_min}m {ttl_sec}s"
                            else:
                                ttl_str = "Expired"
                            
                            # Format adjustment factor with color
                            if adjustment == 1.0:
                                mult_color = RESET
                            elif adjustment > 1.0:
                                mult_color = GREEN
                            else:
                                mult_color = YELLOW
                            
                            mult_str = f"{adjustment:.1f}x"
                            
                            # Format actual multiplier
                            if actual_multiplier:
                                mult_display = f"{actual_multiplier}x"
                                # Color based on value
                                if actual_multiplier <= 10:
                                    mult_display_color = RED  # At minimum
                                elif actual_multiplier >= 900:
                                    mult_display_color = GREEN  # Near maximum
                                else:
                                    mult_display_color = RESET
                            else:
                                mult_display = "?"
                                mult_display_color = RESET
                            
                            print(f"{address:<44} {contract_name:<20} {gas_color}{gas_formatted:<12}{RESET} {ttl_str:<8} {mult_display_color}{mult_display:<12}{RESET} {mult_color}{mult_str:<12}{RESET} {status}")
                            
                            # Add note about actual bounds calculation
                            if address in ["0x940181a94A35A4569E4529A3CDfB74e38FD98631", "0xd0b53D9277642d899DF5C87A3966A349A798F224"]:
                                print(f"{'':>44} {'':>20} {'':>12} {'':>8} {'':>12} Note: If default_value=400, bounds: 80-320,000")
                            print()
                            
                        except ValueError:
                            print(f"{address:<44} {'Error':<20} {'Invalid':<12}")
                
                if count > 0:
                    avg_gas = total_gas / count
                    avg_color = get_gas_color(avg_gas)
                    print("-" * 120)
                    print(f"{BOLD}Average Gas Usage:{RESET} {avg_color}{format_gas(avg_gas)}{RESET}")
                    print(f"\n{BOLD}How bounds work:{RESET}")
                    print("  1. Initial bounds: lower = default_value/5, upper = default_value*1000")
                    print("  2. Upper bound is adjusted by factor shown in 'Next Adjust' column (0.5x to 1.5x)")
                    print("  3. Actual multiplier is clamped between 10x and 1000x")
                    print("  4. Example: If already at 10x min with 0.8x adjustment → stays at 10x")
                    print(f"\n{BOLD}Columns:{RESET}")
                    print("  - Multiplier: Actual current multiplier being used (10x = red/minimum, 900x+ = green/near max)")
                    print("  - Next Adjust: Adjustment factor that will be applied in the next run")
            
            # Show example Redis commands
            print(f"\n{BOLD}Useful Redis Commands:{RESET}")
            print(f"  Check specific contract:  redis-cli -a {redis_password} GET mev:gas:0x...")
            print(f"  Clear all gas history:    redis-cli -a {redis_password} --scan --pattern 'mev:gas:*' | xargs redis-cli -a {redis_password} DEL")
            print(f"  Set test value:          redis-cli -a {redis_password} SETEX mev:gas:0xTEST 3600 35000000")
            
            if continuous:
                print(f"\n{BLUE}Refreshing every 5 seconds... Press Ctrl+C to exit{RESET}")
                time.sleep(5)
            else:
                break
                
    except redis.ConnectionError as e:
        print(f"{RED}Failed to connect to Redis at {redis_host}:{redis_port}{RESET}")
        print(f"Error: {e}")
        print(f"\nMake sure Redis is running and REDIS_LOCAL_PASSWORD is set correctly.")
        return False
    except KeyboardInterrupt:
        print(f"\n{GREEN}Monitoring stopped{RESET}")
        return True
    except Exception as e:
        print(f"{RED}Error: {e}{RESET}")
        return False

def main():
    """Main entry point"""
    import argparse
    
    parser = argparse.ArgumentParser(description="Monitor MEV gas history thresholds in Redis")
    parser.add_argument('-c', '--continuous', action='store_true', 
                       help='Continuously monitor (refresh every 5 seconds)')
    parser.add_argument('-e', '--export', action='store_true',
                       help='Export data in CSV format')
    
    args = parser.parse_args()
    
    if args.export:
        # Export mode - print CSV
        redis_host = os.environ.get('REDIS_LOCAL_HOST', 'localhost')
        redis_port = int(os.environ.get('REDIS_LOCAL_PORT', '6379'))
        redis_password = os.environ.get('REDIS_LOCAL_PASSWORD', '9PmITfN1LiGZDYNr4Gp0LxU9Dq')
        
        try:
            r = redis.Redis(host=redis_host, port=redis_port, password=redis_password, decode_responses=True)
            r.ping()
            
            print("Address,Contract,Gas_Used,Gas_Used_M,TTL_Seconds,Lower_Bound,Upper_Bound,Status")
            
            for key in r.scan_iter(match="mev:gas:*"):
                address = key.replace("mev:gas:", "")
                gas_str = r.get(key)
                ttl = r.ttl(key)
                
                if gas_str:
                    gas_value = int(gas_str)
                    gas_m = gas_value / 1_000_000
                    contract_name = get_contract_name(address)
                    lower, upper, status, adjustment = calculate_bounds_adjustment(gas_value)
                    
                    print(f"{address},{contract_name},{gas_value},{gas_m:.1f},{ttl},{lower},{upper},\"{status}\"")
                    
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            sys.exit(1)
    else:
        # Normal monitoring mode
        monitor_gas_thresholds(continuous=args.continuous)

if __name__ == "__main__":
    main()