#!/bin/bash

# Fix gradient_descent_parallel.rs
sed -i '/OptimizeOutput {/,/}/ {
    /gas_used.*,$/ {
        N
        s/gas_used: \(.*\),\n\([ ]*\)}/gas_used: \1,\n\2filtered_gas: None,\n\2}/
    }
}' crates/mev-base/src/gradient_descent_parallel.rs

# Fix gradient_descent_fast.rs  
sed -i '/OptimizeOutput {/,/}/ {
    /gas_used.*,$/ {
        N
        s/gas_used: \(.*\),\n\([ ]*\)}/gas_used: \1,\n\2filtered_gas: None,\n\2}/
    }
}' crates/mev-base/src/gradient_descent_fast.rs

# Fix gradient_descent_multicall.rs
sed -i '/OptimizeOutput {/,/}/ {
    /gas_used.*,$/ {
        N
        s/gas_used: \(.*\),\n\([ ]*\)}/gas_used: \1,\n\2filtered_gas: None,\n\2}/
    }
}' crates/mev-base/src/gradient_descent_multicall.rs

echo "Fixed OptimizeOutput initializations"