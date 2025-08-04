// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "../contracts/BatchGradientTestV3.sol";
import "../contracts/BatchGradientTestV4.sol";

contract TestGradientOptimizer is Script {
    // Known MEV opportunity examples (you'll need to fill these in)
    uint256 constant KNOWN_BLOCK = 33646198; // Example block
    address constant KNOWN_TARGET = 0x0000000000000000000000000000000000000000; // Fill in
    uint256 constant KNOWN_PROFITABLE_QTY = 1000; // Fill in
    
    function run() public {
        // Fork at the specific block
        vm.createSelectFork(vm.envString("RPC_URL"), KNOWN_BLOCK);
        
        console.log("Forked at block:", KNOWN_BLOCK);
        console.log("Testing target:", KNOWN_TARGET);
        
        // Deploy test contracts
        BatchGradientTestV3 v3 = new BatchGradientTestV3();
        BatchGradientTestV4 v4 = new BatchGradientTestV4();
        
        console.log("V3 deployed at:", address(v3));
        console.log("V4 deployed at:", address(v4));
        
        // Test parameters
        uint256 lowerBound = 1;
        uint256 upperBound = 1000000;
        uint256 maxIterations = 75;
        uint256 initialValue = KNOWN_PROFITABLE_QTY;
        
        // Test V3
        console.log("\n--- Testing V3 ---");
        BatchGradientTestV3.BinarySearchResult memory resultV3 = v3.binarySearch(
            KNOWN_TARGET,
            lowerBound,
            upperBound,
            maxIterations,
            initialValue
        );
        
        console.log("V3 Results:");
        console.log("  Best Quantity:", resultV3.bestQuantity);
        console.log("  Best Profit:", resultV3.bestProfit);
        console.log("  Tests Performed:", resultV3.testsPerformed);
        
        // Test V4 with more iterations
        console.log("\n--- Testing V4 ---");
        maxIterations = 210;
        BatchGradientTestV3.BinarySearchResult memory resultV4 = BatchGradientTestV3(address(v4)).binarySearch(
            KNOWN_TARGET,
            lowerBound,
            upperBound,
            maxIterations,
            initialValue
        );
        
        console.log("V4 Results:");
        console.log("  Best Quantity:", resultV4.bestQuantity);
        console.log("  Best Profit:", resultV4.bestProfit);
        console.log("  Tests Performed:", resultV4.testsPerformed);
        
        // Direct test of known profitable quantity
        console.log("\n--- Direct Test of Known Quantity ---");
        testSingleQuantity(KNOWN_TARGET, KNOWN_PROFITABLE_QTY);
        
        // Test a range around the known quantity
        console.log("\n--- Testing Range Around Known Quantity ---");
        uint256 start = KNOWN_PROFITABLE_QTY > 100 ? KNOWN_PROFITABLE_QTY - 100 : 1;
        uint256 end = KNOWN_PROFITABLE_QTY + 100;
        
        for (uint256 i = start; i <= end && i < start + 10; i++) {
            testSingleQuantity(KNOWN_TARGET, i);
        }
    }
    
    function testSingleQuantity(address target, uint256 quantity) internal {
        // Encode the calldata: 0x00 followed by the last 3 bytes of quantity
        bytes memory callData = abi.encodePacked(
            bytes1(0x00),
            bytes3(uint24(quantity))
        );
        
        // Try the call
        (bool success, bytes memory returnData) = target.call(callData);
        
        if (!success && returnData.length >= 32) {
            int256 profit = abi.decode(returnData, (int256));
            console.log("Quantity", quantity, "profit:", profit);
        } else if (success) {
            console.log("Quantity", quantity, "succeeded (no profit)");
        } else {
            console.log("Quantity", quantity, "failed with no data");
        }
    }
}