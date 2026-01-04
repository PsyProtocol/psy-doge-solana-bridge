#!/bin/bash
set -e

# Define the programs to build
PROGRAMS=(
    "doge-bridge"
    "manual-claim"
    "pending-mint-buffer"
    "txo-buffer"
    "generic-buffer"
)

echo "------------------------------------------------"
echo "Building Solana Programs (SBF)"
echo "------------------------------------------------"

# Construct the package flags
PACKAGE_FLAGS="--features solprogram --features mock-zkp --no-default-features"
for prog in "${PROGRAMS[@]}"; do
    PACKAGE_FLAGS="$PACKAGE_FLAGS -p $prog" 
done

# Run the build command
# The '--' ensures that -p flags are passed to the underlying cargo build command
# and not interpreted by cargo-build-sbf wrapper itself incorrectly.
cargo build-sbf -- $PACKAGE_FLAGS

echo ""
echo "------------------------------------------------"
echo "Build Complete."
echo "------------------------------------------------"