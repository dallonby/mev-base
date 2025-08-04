#!/bin/bash

# Add filtered_gas to gradient_descent_parallel.rs
sed -i 's/gas_used,$/gas_used,\n                            filtered_gas: None,/g' crates/mev-base/src/gradient_descent_parallel.rs

# Add filtered_gas to gradient_descent_fast.rs  
sed -i 's/gas_used,$/gas_used,\n                            filtered_gas: None,/g' crates/mev-base/src/gradient_descent_fast.rs

echo "Fixed remaining OptimizeOutput"