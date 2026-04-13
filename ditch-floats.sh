#!/bin/bash

cargo check -p solana-bloom --features agave-unstable-api
cargo check -p solana-gossip
cargo test -p solana-bloom --features agave-unstable-api test_filter_math -- --nocapture
cargo test -p solana-bloom --features agave-unstable-api test_bloom_wire_format_regression -- --nocapture
cargo test -p solana-gossip test_crds_filter_mask -- --nocapture
