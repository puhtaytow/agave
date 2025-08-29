#!/usr/bin/env python3
#
# ./find-disallowed-ports.py 2>&1 | tee output.txt
# cargo +nightly-2025-02-16 nextest run --test-threads 1
# cargo +nightly-2025-02-16 nextest run --workspace --test-threads 1 \
#   --exclude solana-cargo-build-sbf \
#   --exclude  solana-cli \
#   --exclude solana-bench-tps

import subprocess
import sys
import time
import re
import datetime

# Allowed port range
MIN_PORT = 2000
MAX_PORT = 3000

# Command to run
CMD = ["ss", "-O", "-H", "-p", "-n", "-u", "-a"]

# Regex to match UDP ports, e.g. "127.0.0.1:2021" or "*:2021"
PORT_REGEX = re.compile(r":(\d+)$")

IGNORE_PORTS = [
    1489,    # rpc +1
    1488,    # base port -512,
    34259, # docker
    ###
    1490, # for test solana-dos / vortextor
    1491, # vortextor
    1492,

]
# IGNORE_PORTS = []

def get_udp_ports():
    try:
        output = subprocess.check_output(CMD, text=True)
    except subprocess.CalledProcessError as e:
        print(f"Error running ss: {e}", file=sys.stderr)
        sys.exit(2)

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

def main():
    print("Started")
    sys.stdout.flush()
    while True:
        ports = get_udp_ports()
        for port in ports:
            # ignore below 1024
            if port<(1024):
                continue
            # ignore from list
            if port in IGNORE_PORTS:
                continue
            # #
            if port < MIN_PORT or port > MAX_PORT:
                timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                print(f"[{timestamp}] ERROR: Detected disallowed UDP port {port}")
                subprocess.call("killall cargo-nextest", shell= True)

###
if __name__ == "__main__":
    main()