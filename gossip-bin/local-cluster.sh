#!/bin/bash

# BOOTNODE
# cargo run -- --allow-private-addr spy --bind-address 127.0.0.1 --gossip-port 8001 --shred-version 1

HOW_MANY=100

seq 1 $HOW_MANY | parallel -j $HOW_MANY 'cargo run -- --allow-private-addr spy --entrypoint 127.0.0.1:8001 --bind-address 127.0.0.1 --gossip-port $((8001 + {})) --shred-version 1'