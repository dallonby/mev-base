// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

contract BatchGradientTest {
    struct TestResult {
        bool success;
        bytes returnData;
        uint256 gasUsed;
    }
    
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
}