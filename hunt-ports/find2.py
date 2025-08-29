#!/usr/bin/env python3
#
# ./find-disallowed-ports-enhanced.py 2>&1 | tee output.txt
# Enhanced version that runs cargo tests and logs disallowed ports
import subprocess
import sys
import time
import re
import datetime
import os

# Allowed port range
MIN_PORT = 2000
MAX_PORT = 3000

# Commands
SS_CMD = ["ss", "-O", "-H", "-p", "-n", "-u", "-a"]
CARGO_CMD = ["cargo", "+nightly-2025-02-16", "nextest", "run", "--test-threads", "1", "--no-capture"]

# Regex to match UDP ports, e.g. "127.0.0.1:2021" or "*:2021"
PORT_REGEX = re.compile(r":(\d+)$")

IGNORE_PORTS = [
    1488,   # base port -512,
    53,     # DNS (127.0.0.54:53, 127.0.0.53:53)
    36017,  # UDP (0.0.0.0:36017)
    5353,   # mDNS (0.0.0.0:5353, [::]:5353)
    51410,  # UDP (*:51410)
    55197,  # UDP ([::]:55197)
    22,     # SSH (0.0.0.0:22, [::]:22)
    39441,  # TCP (127.0.0.1:39441)
    631     # IPP/CUPS (127.0.0.1:631, [::1]:631)
]

# Runtime ignored ports (discovered during execution)
runtime_ignored_ports = set()

def get_udp_ports():
    """Get list of currently used UDP ports"""
    try:
        output = subprocess.check_output(SS_CMD, text=True)
    except subprocess.CalledProcessError as e:
        print(f"Error running ss: {e}", file=sys.stderr)
        return []
    
    ports = []
    for line in output.strip().split("\n"):
        if not line.strip():
            continue
        # The local address:port is usually the 5th field in ss output
        fields = line.split()
        if len(fields) < 4:
            continue
        local_addr = fields[3]
        match = PORT_REGEX.search(local_addr)
        if match:
            ports.append(int(match.group(1)))
    return ports

def save_cargo_log(port, cargo_output):
    """Save cargo nextest output to timestamped log file"""
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    log_filename = f"cargo_nextest_disallowed_port_{port}_{timestamp}.log"
    
    try:
        with open(log_filename, 'w') as f:
            f.write(f"Disallowed port detected: {port}\n")
            f.write(f"Timestamp: {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
            f.write("=" * 80 + "\n")
            f.write("CARGO NEXTEST OUTPUT:\n")
            f.write("=" * 80 + "\n")
            f.write(cargo_output)
        print(f"Cargo log saved to: {log_filename}")
    except Exception as e:
        print(f"Error saving cargo log: {e}", file=sys.stderr)

def run_cargo_tests():
    """Run cargo nextest and return output"""
    print("Running cargo nextest...")
    try:
        # Capture both stdout and stderr
        result = subprocess.run(
            CARGO_CMD, 
            capture_output=True, 
            text=True, 
            timeout=300  # 5 minute timeout
        )
        return result.stdout + "\n" + result.stderr
    except subprocess.TimeoutExpired:
        print("Cargo nextest timed out after 5 minutes")
        return "ERROR: Cargo nextest timed out after 5 minutes"
    except subprocess.CalledProcessError as e:
        return f"ERROR: Cargo nextest failed with return code {e.returncode}\n{e.stdout}\n{e.stderr}"
    except Exception as e:
        return f"ERROR: Failed to run cargo nextest: {e}"

def main():
    print("Enhanced port monitor started")
    print(f"Monitoring for ports outside range {MIN_PORT}-{MAX_PORT}")
    print(f"Initially ignored ports: {sorted(IGNORE_PORTS)}")
    sys.stdout.flush()
    
    iteration = 0
    
    while True:
        iteration += 1
        print(f"\n--- Iteration {iteration} ---")
        
        # Run cargo tests first
        cargo_output = run_cargo_tests()
        
        # Check for disallowed ports
        ports = get_udp_ports()
        disallowed_ports = []
        
        for port in ports:
            # Skip ports below 1024
            if port < 1024:
                continue
            
            # Skip initially ignored ports
            if port in IGNORE_PORTS:
                continue
                
            # Skip runtime ignored ports
            if port in runtime_ignored_ports:
                continue
            
            # Check if port is outside allowed range
            if port < MIN_PORT or port > MAX_PORT:
                disallowed_ports.append(port)
        
        # Process disallowed ports
        if disallowed_ports:
            timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            for port in disallowed_ports:
                print(f"[{timestamp}] ERROR: Detected disallowed UDP port {port}")
                
                # Save cargo log
                save_cargo_log(port, cargo_output)
                
                # Add to runtime ignored ports
                runtime_ignored_ports.add(port)
                print(f"Port {port} added to runtime ignore list")
            
            print(f"Runtime ignored ports: {sorted(runtime_ignored_ports)}")
        else:
            print(f"[{datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] All ports in allowed range")
        
        # Wait before next iteration
        print("Waiting 10 seconds before next check...")
        time.sleep(10)

if __name__ == "__main__":
    main()