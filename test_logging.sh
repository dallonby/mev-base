#!/bin/bash

echo "Testing MEV logging system with different verbosity levels..."
echo

echo "1. Testing ERROR level (MEV_LOG=error)"
echo "   Should only show errors"
echo "   Run: MEV_LOG=error cargo run -p mevbase --profile=release"
echo

echo "2. Testing WARN level (MEV_LOG=warn)"
echo "   Should show warnings and errors"
echo "   Run: MEV_LOG=warn cargo run -p mevbase --profile=release"
echo

echo "3. Testing INFO level (MEV_LOG=info)"
echo "   Should show info, warnings and errors (default)"
echo "   Run: MEV_LOG=info cargo run -p mevbase --profile=release"
echo

echo "4. Testing DEBUG level (MEV_LOG=debug)"
echo "   Should show debug messages and above"
echo "   Run: MEV_LOG=debug cargo run -p mevbase --profile=release"
echo

echo "5. Testing TRACE level (MEV_LOG=trace)"
echo "   Should show all messages including trace"
echo "   Run: MEV_LOG=trace cargo run -p mevbase --profile=release"
echo

echo "6. Testing module-specific filtering"
echo "   Example: MEV_LOG=info,mevbase::mev_task_worker=debug"
echo "   Shows info globally but debug for mev_task_worker module"
echo

echo "You can also create a .env file with MEV_LOG=<level> to set the default"