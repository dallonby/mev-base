// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

contract BatchGradientTestV4 {
    struct TestResult {
        bool success;
        bytes returnData;
        uint256 gasUsed;
    }
    
    struct BinarySearchResult {
        uint256 bestQuantity;
        int256 bestProfit;
        uint256 testsPerformed;
    }
    
    // Original batch test method
    function batchTest(
        address target,
        uint256[] calldata quantities
    ) external returns (TestResult[] memory results) {
        uint256 length = quantities.length;
        results = new TestResult[](length);
        
        for (uint256 i = 0; i < length; i++) {
            uint256 qty = quantities[i];
            
            // Encode the calldata: 0x00 followed by the last 3 bytes of quantity
            bytes memory callData = abi.encodePacked(
                bytes1(0x00),
                bytes3(uint24(qty))
            );
            
            // Record gas before call
            uint256 gasBefore = gasleft();
            
            // Call the target contract
            (bool success, bytes memory returnData) = target.call(callData);
            
            // Calculate gas used
            uint256 gasUsed = gasBefore - gasleft();
            
            // Store the result
            results[i] = TestResult({
                success: success,
                returnData: returnData,
                gasUsed: gasUsed
            });
        }
        
        return results;
    }
    
    // Hybrid search: logarithmic + random initial scan, then binary search
    function binarySearch(
        address target,
        uint256 lowerBound,
        uint256 upperBound,
        uint256 maxIterations,
        uint256 initialValue
    ) external returns (BinarySearchResult memory result) {
        result.bestQuantity = lowerBound;
        result.bestProfit = 0;
        
        uint256 left;
        uint256 right;
        int256 profit;
        
        // Phase 1: Hybrid initial scan (60 iterations)
        // Search range: initialValue/10 to initialValue*1000
        left = initialValue / 10;  // Reuse left as scanMin
        right = initialValue * 1000; // Reuse right as scanMax
        
        // Ensure bounds
        if (left < lowerBound) left = lowerBound;
        if (right > upperBound) right = upperBound;
        
        // Use 40% of iterations for initial scan (30 samples for 75 iterations)
        uint256 totalInitialScan = maxIterations * 2 / 5; // 40% for initial scan
        uint256 logSamples = totalInitialScan * 2 / 3;     // 2/3 logarithmic
        if (logSamples > 40) logSamples = 40;              // Cap at 40
        
        // Store scanMin and scanMax in temporary vars before loop
        uint256 scanMin = left;
        uint256 scanMax = right;
        
        // Logarithmic sampling across the range
        for (uint256 i = 0; i < logSamples && result.testsPerformed < maxIterations; i++) {
            // Calculate logarithmic position
            // We want to sample from log(scanMin) to log(scanMax)
            // Since we can't do floating point, we'll use a multiplier approach
            uint256 testQty;
            if (i == 0) {
                testQty = initialValue / 10; // 0.1x
            } else if (i < 10) {
                // 0.1x to 1x range (more samples in lower range)
                testQty = (initialValue * (10 + (i * 90) / 10)) / 100;
            } else if (i < 25) {
                // 1x to 10x range
                testQty = (initialValue * (100 + ((i - 10) * 900) / 15)) / 100;
            } else {
                // 10x to 1000x range
                testQty = (initialValue * (1000 + ((i - 25) * 99000) / 15)) / 100;
            }
            
            // Ensure within bounds
            if (testQty < scanMin) testQty = scanMin;
            if (testQty > scanMax) testQty = scanMax;
            
            if (testQty >= lowerBound && testQty <= upperBound) {
                profit = testSingleQuantity(target, testQty);
                result.testsPerformed++;
                if (profit > result.bestProfit) {
                    result.bestProfit = profit;
                    result.bestQuantity = testQty;
                }
            }
        }
        
        // Remaining initial scan samples are random
        uint256 randomSamples = totalInitialScan - logSamples;
        if (randomSamples > 20) randomSamples = 20; // Cap at 20
        
        // Simple pseudo-random using block data
        uint256 seed = uint256(keccak256(abi.encodePacked(block.timestamp, block.prevrandao, target)));
        
        for (uint256 i = 0; i < randomSamples && result.testsPerformed < maxIterations; i++) {
            // Update seed for next random
            seed = uint256(keccak256(abi.encodePacked(seed, i)));
            
            // Generate random value in range
            uint256 range = scanMax - scanMin;
            uint256 randomOffset = seed % range;
            uint256 testQty = scanMin + randomOffset;
            
            if (testQty >= lowerBound && testQty <= upperBound) {
                profit = testSingleQuantity(target, testQty);
                result.testsPerformed++;
                if (profit > result.bestProfit) {
                    result.bestProfit = profit;
                    result.bestQuantity = testQty;
                }
            }
        }
        
        // If no profit found in initial scan, return early
        if (result.bestProfit == 0) {
            return result;
        }
        
        // Phase 2: Binary search around best found (remaining budget: ~150 iterations)
        // Set search bounds around the best found quantity
        profit = int256(result.bestQuantity / 2); // Reuse profit as searchRadius
        left = result.bestQuantity > uint256(profit) ? result.bestQuantity - uint256(profit) : scanMin;
        right = result.bestQuantity + uint256(profit) < scanMax ? result.bestQuantity + uint256(profit) : scanMax;
        
        // Binary search with remaining iterations
        uint256 mid;
        for (uint256 i = result.testsPerformed; i < maxIterations && left < right; i++) {
            mid = (left + right) / 2;
            
            // Test the midpoint
            profit = testSingleQuantity(target, mid);
            result.testsPerformed++;
            
            if (profit > result.bestProfit) {
                result.bestProfit = profit;
                result.bestQuantity = mid;
            }
            
            // Test neighbors
            int256 leftProfit = 0;
            if (mid > lowerBound && result.testsPerformed < maxIterations) {
                leftProfit = testSingleQuantity(target, mid - 1);
                result.testsPerformed++;
                if (leftProfit > result.bestProfit) {
                    result.bestProfit = leftProfit;
                    result.bestQuantity = mid - 1;
                }
            }
            
            int256 rightProfit = 0;
            if (mid < upperBound && result.testsPerformed < maxIterations) {
                rightProfit = testSingleQuantity(target, mid + 1);
                result.testsPerformed++;
                if (rightProfit > result.bestProfit) {
                    result.bestProfit = rightProfit;
                    result.bestQuantity = mid + 1;
                }
            }
            
            // Decide direction
            if (leftProfit > rightProfit) {
                right = mid - 1;
            } else if (rightProfit > leftProfit) {
                left = mid + 1;
            } else {
                // Local maximum or plateau, try wider search
                uint256 widerRange = (right - left) / 4;
                if (widerRange > 0 && result.testsPerformed + 2 < maxIterations) {
                    // Test wider points
                    if (mid > lowerBound + widerRange) {
                        profit = testSingleQuantity(target, mid - widerRange);
                        result.testsPerformed++;
                        if (profit > result.bestProfit) {
                            result.bestProfit = profit;
                            result.bestQuantity = mid - widerRange;
                            right = mid;
                            continue;
                        }
                    }
                    
                    if (mid < upperBound - widerRange) {
                        profit = testSingleQuantity(target, mid + widerRange);
                        result.testsPerformed++;
                        if (profit > result.bestProfit) {
                            result.bestProfit = profit;
                            result.bestQuantity = mid + widerRange;
                            left = mid;
                            continue;
                        }
                    }
                }
                
                // No better profit found, we're done
                break;
            }
        }
        
        return result;
    }
    
    // Helper function to test a single quantity and extract profit
    function testSingleQuantity(address target, uint256 quantity) internal returns (int256) {
        // Encode the calldata: 0x00 followed by the last 3 bytes of quantity
        bytes memory callData = abi.encodePacked(
            bytes1(0x00),
            bytes3(uint24(quantity))
        );
        
        // Call the target contract with 4M gas limit
        (bool success, bytes memory returnData) = target.call{gas: 4000000}(callData);
        
        if (success) {
            // Contract succeeded - no profit
            return 0;
        } else if (returnData.length >= 32) {
            // Extract profit from revert data
            int256 profit;
            assembly {
                profit := mload(add(returnData, 0x20))
            }
            return profit;
        } else {
            return 0;
        }
    }
}