// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "forge-std/console2.sol";
import "../contracts/BatchGradientTestV3.sol";
import "../contracts/BatchGradientTestV4.sol";

contract TestGradientOptimizerTx is Script {
    // Transaction: 0x27f99aba5191627528ff99cd5741f2b050c67b044b0e50ffeb6fb0c6a487717e
    // Block: 33702330
    // Target: 0x64aaFD7d9167de080C6f58849fbc11F16E6ca32C
    // Input: 0x00000266 (quantity = 614)
    
    function run() public {
        uint256 blockNumber = 33702330;
        address targetContract = 0x64aaFD7d9167de080C6f58849fbc11F16E6ca32C;
        uint256 originalQuantity = 0x266; // 614 in decimal
        
        console2.log("Testing optimizers against known MEV transaction");
        console2.log("Transaction: 0x27f99aba5191627528ff99cd5741f2b050c67b044b0e50ffeb6fb0c6a487717e");
        console2.log("Block:", blockNumber);
        console2.log("Target:", targetContract);
        console2.log("Original quantity:", originalQuantity);
        
        // Fork at the block of the transaction
        vm.createSelectFork("http://127.0.0.1:28545", blockNumber);
        
        console2.log("\nForked at block:", blockNumber);
        
        // Roll to the state right before our target transaction
        bytes32 txHash = 0x27f99aba5191627528ff99cd5741f2b050c67b044b0e50ffeb6fb0c6a487717e;
        vm.rollFork(txHash);
        console2.log("Rolled fork to transaction: 0x27f99aba5191627528ff99cd5741f2b050c67b044b0e50ffeb6fb0c6a487717e");
        
        // Set gas price to 0 for simulations
        vm.txGasPrice(0);
        console2.log("Set gas price to 0 for simulations");
        
        // Fund our bot address with ETH for testing
        address botAddress = 0x3a3F76931108c79658A90F340b4CbEC860346b2B;
        vm.deal(botAddress, 10 ether);
        console2.log("Funded bot address with 10 ETH:", botAddress);
        
        // Deploy test contracts
        BatchGradientTestV3 v3 = new BatchGradientTestV3();
        BatchGradientTestV4 v4 = new BatchGradientTestV4();
        
        console2.log("\nTest contracts deployed:");
        console2.log("V3:", address(v3));
        console2.log("V4:", address(v4));
        
        // First, verify the original transaction would have been profitable
        console2.log("\n--- Verifying Original Transaction ---");
        testSingleQuantity(targetContract, originalQuantity);
        
        // Test V3 optimizer (the one that was working)
        console2.log("\n--- Testing V3 Optimizer ---");
        uint256 lowerBound = 1;
        uint256 upperBound = originalQuantity * 100; // Search up to 100x
        uint256 maxIterationsV3 = 75;
        
        BatchGradientTestV3.BinarySearchResult memory resultV3 = v3.binarySearch(
            targetContract,
            lowerBound,
            upperBound,
            maxIterationsV3,
            originalQuantity
        );
        
        console2.log("V3 Results:");
        console2.log("  Best Quantity:", resultV3.bestQuantity);
        console2.log("  Best Profit:", resultV3.bestProfit);
        console2.log("  Tests Performed:", resultV3.testsPerformed);
        
        if (resultV3.bestProfit > 0) {
            console2.log("  [SUCCESS] V3 found profit!");
            console2.log("  Verifying V3's best quantity:");
            testSingleQuantity(targetContract, resultV3.bestQuantity);
        } else {
            console2.log("  [FAILED] V3 found no profit");
        }
        
        // Test V4 optimizer (the new one that's not finding opportunities)
        console2.log("\n--- Testing V4 Optimizer ---");
        uint256 maxIterationsV4 = 210;
        
        BatchGradientTestV3.BinarySearchResult memory resultV4 = BatchGradientTestV3(address(v4)).binarySearch(
            targetContract,
            lowerBound,
            upperBound,
            maxIterationsV4,
            originalQuantity
        );
        
        console2.log("V4 Results:");
        console2.log("  Best Quantity:", resultV4.bestQuantity);
        console2.log("  Best Profit:", resultV4.bestProfit);
        console2.log("  Tests Performed:", resultV4.testsPerformed);
        
        if (resultV4.bestProfit > 0) {
            console2.log("  [SUCCESS] V4 found profit!");
            console2.log("  Verifying V4's best quantity:");
            testSingleQuantity(targetContract, resultV4.bestQuantity);
        } else {
            console2.log("  [FAILED] V4 found no profit");
        }
        
        // Test the specific multiples/divisions that V3 uses
        console2.log("\n--- Testing V3's Specific Search Pattern ---");
        console2.log("V3 tests these specific values:");
        testSingleQuantity(targetContract, originalQuantity);      // 1x = 614
        testSingleQuantity(targetContract, originalQuantity / 2);  // 0.5x = 307
        testSingleQuantity(targetContract, originalQuantity / 4);  // 0.25x = 153
        testSingleQuantity(targetContract, originalQuantity / 6);  // 0.167x = 102
        testSingleQuantity(targetContract, originalQuantity * 2);  // 2x = 1228
        testSingleQuantity(targetContract, originalQuantity * 4);  // 4x = 2456
        testSingleQuantity(targetContract, originalQuantity * 6);  // 6x = 3684
        
        // Test a wider range to find if any profitable opportunities exist
        console2.log("\n--- Testing Wide Range ---");
        uint256[] memory testQuantities = new uint256[](10);
        testQuantities[0] = 10;
        testQuantities[1] = 50;
        testQuantities[2] = 100;
        testQuantities[3] = 200;
        testQuantities[4] = 500;
        testQuantities[5] = 1000;
        testQuantities[6] = 2000;
        testQuantities[7] = 5000;
        testQuantities[8] = 10000;
        testQuantities[9] = 20000;
        
        for (uint256 i = 0; i < testQuantities.length; i++) {
            testSingleQuantity(targetContract, testQuantities[i]);
        }
    }
    
    function testSingleQuantity(address target, uint256 qty) internal {
        if (qty == 0) return;
        
        // Encode the calldata: 0x00 followed by the last 3 bytes of quantity
        bytes memory callData = abi.encodePacked(
            bytes1(0x00),
            bytes3(uint24(qty))
        );
        
        // Try the call with value (the original tx sent 4875289076 wei)
        uint256 msgValue = 4875289076;
        
        // Use vm.prank to call from a non-c0ffee address (our test bot address)
        address botAddress = 0x3a3F76931108c79658A90F340b4CbEC860346b2B;
        vm.prank(botAddress);
        (bool success, bytes memory returnData) = target.call{value: msgValue}(callData);
        
        if (!success && returnData.length >= 32) {
            int256 profit = abi.decode(returnData, (int256));
            console2.log("Qty:", qty);
            console2.log("  -> Profit:", profit);
            if (profit > 0) {
                console2.log("  >>> PROFITABLE! <<<");
            }
        } else if (success) {
            console2.log("Qty:", qty);
            console2.log("  -> Success (no revert, no profit)");
        } else {
            console2.log("Qty:", qty);
            console2.log("  -> Failed, returnData length:");
            console2.log(returnData.length);
        }
    }
}