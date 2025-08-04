#!/bin/bash

echo "Debugging MEV hang issue"
echo "======================="

# 1. Check if Redis is responsive
echo -e "\n1. Testing Redis connection..."
if timeout 2 redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq ping > /dev/null 2>&1; then
    echo "✓ Redis is responsive"
else
    echo "✗ Redis is not responding or auth failed"
fi

# 2. Check if process is running
echo -e "\n2. Checking mevbase process..."
PID=$(pgrep -f "mevbase" | head -1)
if [ -n "$PID" ]; then
    echo "✓ Process found: PID $PID"
    
    # Check CPU usage
    CPU=$(ps -p $PID -o %cpu= | tr -d ' ')
    echo "  CPU usage: ${CPU}%"
    
    # Check process state
    STATE=$(ps -p $PID -o state= | tr -d ' ')
    echo "  Process state: $STATE (S=sleeping, R=running, D=disk wait)"
    
    # Check open files/connections
    echo -e "\n3. Checking open connections..."
    CONNECTIONS=$(lsof -p $PID 2>/dev/null | grep -E "TCP|PIPE|REG" | wc -l)
    echo "  Open file descriptors: $CONNECTIONS"
    
    # Check if stuck on Redis
    REDIS_CONN=$(lsof -p $PID 2>/dev/null | grep -i redis | wc -l)
    echo "  Redis connections: $REDIS_CONN"
    
    # Get stack trace if possible
    echo -e "\n4. Attempting to get stack trace..."
    if command -v gdb > /dev/null 2>&1; then
        timeout 2 gdb -p $PID -batch -ex "thread apply all bt" 2>/dev/null | head -50
    else
        echo "  GDB not available, trying strace..."
        timeout 2 strace -p $PID -c 2>&1 | head -20
    fi
else
    echo "✗ No mevbase process found"
fi

# 3. Check Redis for blocking operations
echo -e "\n5. Checking Redis for blocking operations..."
redis-cli -a 9PmITfN1LiGZDYNr4Gp0LxU9Dq CLIENT LIST 2>/dev/null | grep -E "cmd=|blocked" | head -5

# 4. Check system resources
echo -e "\n6. System resources:"
echo "  Memory: $(free -h | grep Mem | awk '{print $3 "/" $2}')"
echo "  Load: $(uptime | awk -F'load average:' '{print $2}')"

# 5. Check logs
echo -e "\n7. Recent errors in syslog:"
journalctl -u mevbase -n 20 --no-pager 2>/dev/null || dmesg | tail -20 | grep -i error

echo -e "\n8. Recommendations:"
echo "  - If hung on Redis: Check REDIS_LOCAL_* env vars are set"
echo "  - If high CPU: May be in infinite loop in optimizer"
echo "  - If blocked on I/O: Check disk space and network"
echo "  - Try: killall -SIGUSR1 mevbase (to dump stack trace if supported)"