// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "forge-std/console2.sol";
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
        
        console2.log("Forked at block:", KNOWN_BLOCK);
        console2.log("Testing target:", KNOWN_TARGET);
        
        // Deploy test contracts
        BatchGradientTestV3 v3 = new BatchGradientTestV3();
        BatchGradientTestV4 v4 = new BatchGradientTestV4();
        
        console2.log("V3 deployed at:", address(v3));
        console2.log("V4 deployed at:", address(v4));
        
        // Test parameters
        uint256 lowerBound = 1;
        uint256 upperBound = 1000000;
        uint256 maxIterations = 75;
        uint256 initialValue = KNOWN_PROFITABLE_QTY;
        
        // Test V3
        console2.log("\n--- Testing V3 ---");
        BatchGradientTestV3.BinarySearchResult memory resultV3 = v3.binarySearch(
            KNOWN_TARGET,
            lowerBound,
            upperBound,
            maxIterations,
            initialValue
        );
        
        console2.log("V3 Results:");
        console2.log("  Best Quantity:", resultV3.bestQuantity);
        console2.log("  Best Profit:", resultV3.bestProfit);
        console2.log("  Tests Performed:", resultV3.testsPerformed);
        
        // Test V4 with more iterations
        console2.log("\n--- Testing V4 ---");
        maxIterations = 210;
        BatchGradientTestV3.BinarySearchResult memory resultV4 = BatchGradientTestV3(address(v4)).binarySearch(
            KNOWN_TARGET,
            lowerBound,
            upperBound,
            maxIterations,
            initialValue
        );
        
        console2.log("V4 Results:");
        console2.log("  Best Quantity:", resultV4.bestQuantity);
        console2.log("  Best Profit:", resultV4.bestProfit);
        console2.log("  Tests Performed:", resultV4.testsPerformed);
        
        // Direct test of known profitable quantity
        console2.log("\n--- Direct Test of Known Quantity ---");
        testSingleQuantity(KNOWN_TARGET, KNOWN_PROFITABLE_QTY);
        
        // Test a range around the known quantity
        console2.log("\n--- Testing Range Around Known Quantity ---");
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
            console2.log("Quantity:", quantity);
            console2.log("Profit:", profit);
        } else if (success) {
            console2.log("Quantity succeeded (no profit):", quantity);
        } else {
            console2.log("Quantity failed with no data:", quantity);
        }
    }
}