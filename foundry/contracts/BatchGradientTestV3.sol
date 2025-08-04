// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

contract BatchGradientTestV3 {
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
    
    // Specific multiples/divisions search followed by binary search
    function binarySearch(
        address target,
        uint256 lowerBound,
        uint256 upperBound,
        uint256 maxIterations,
        uint256 initialValue
    ) external returns (BinarySearchResult memory result) {
        result.bestQuantity = lowerBound;
        result.bestProfit = 0;
        
        uint256 left = lowerBound;
        uint256 right = upperBound;
        uint256 mid;
        int256 profit;
        
        // Phase 1: Test specific multiples and divisions of default_value
        uint256[7] memory testValues;
        testValues[0] = initialValue;           // default_value
        testValues[1] = initialValue / 2;       // default_value/2
        testValues[2] = initialValue / 4;       // default_value/4
        testValues[3] = initialValue / 6;       // default_value/6
        testValues[4] = initialValue * 2;       // default_value*2
        testValues[5] = initialValue * 4;       // default_value*4
        testValues[6] = initialValue * 6;       // default_value*6
        
        // Test each value if within bounds
        for (uint256 i = 0; i < 7 && result.testsPerformed < maxIterations; i++) {
            uint256 testQty = testValues[i];
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
        
        // Phase 2: Binary search around best found
        // Set search bounds around the best found quantity
        uint256 range = upperBound - lowerBound;
        uint256 searchRadius = range / 10; // 10% of range
        left = result.bestQuantity > searchRadius ? result.bestQuantity - searchRadius : lowerBound;
        right = result.bestQuantity + searchRadius < upperBound ? result.bestQuantity + searchRadius : upperBound;
        
        // Binary search with remaining iterations
        for (uint256 i = result.testsPerformed; i < maxIterations && left < right; i++) {
            mid = (left + right) / 2;
            
            // Test the midpoint
            profit = testSingleQuantity(target, mid);
            result.testsPerformed++;
            
            if (profit > result.bestProfit) {
                result.bestProfit = profit;
                result.bestQuantity = mid;
            }
            
            // Reuse profit variable for left test
            profit = 0;
            if (mid > lowerBound) {
                profit = testSingleQuantity(target, mid - 1);
                result.testsPerformed++;
            }
            
            // Store left profit, test right
            int256 leftProfit = profit;
            profit = 0;
            if (mid < upperBound) {
                profit = testSingleQuantity(target, mid + 1);
                result.testsPerformed++;
            }
            
            // Decide which direction to go (profit now contains rightProfit)
            if (leftProfit > result.bestProfit && leftProfit > profit) {
                // Go left
                right = mid - 1;
            } else if (profit > result.bestProfit && profit > leftProfit) {
                // Go right
                left = mid + 1;
            } else {
                // We're at a local maximum, do a wider search
                mid = (right - left) / 4; // Reuse mid as range
                if (mid > 0) {
                    // Test wider points, reusing profit variable
                    if ((left + right) / 2 > lowerBound + mid) {
                        profit = testSingleQuantity(target, (left + right) / 2 - mid);
                        result.testsPerformed++;
                        if (profit > result.bestProfit) {
                            result.bestProfit = profit;
                            result.bestQuantity = (left + right) / 2 - mid;
                            right = (left + right) / 2;
                            left = (left + right) / 2 - mid * 2;
                            if (left < lowerBound) left = lowerBound;
                            continue;
                        }
                    }
                    
                    if ((left + right) / 2 < upperBound - mid) {
                        profit = testSingleQuantity(target, (left + right) / 2 + mid);
                        result.testsPerformed++;
                        if (profit > result.bestProfit) {
                            result.bestProfit = profit;
                            result.bestQuantity = (left + right) / 2 + mid;
                            left = (left + right) / 2;
                            right = (left + right) / 2 + mid * 2;
                            if (right > upperBound) right = upperBound;
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
        
        // Call the target contract
        (bool success, bytes memory returnData) = target.call(callData);
        
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