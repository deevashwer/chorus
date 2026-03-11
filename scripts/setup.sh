#!/usr/bin/env bash
# Set up a fresh Ubuntu VM for Chorus benchmarks.
# Installs Rust, C/C++ dependencies, builds the artifact, and generates
# benchmark state.  Idempotent — safe to run multiple times.
#
# Usage:  cd ~/chorus && bash scripts/setup.sh
set -e

echo "=== Chorus VM Setup ==="

# System packages
echo "Installing system packages..."
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    libgmp-dev libmpfr-dev libssl-dev m4 build-essential pkg-config \
    cmake python3 curl

# Rust
if ! command -v cargo &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
. "$HOME/.cargo/env"

# Build
python3 scripts/run.py build

# Generate benchmark state
python3 scripts/run.py generate

echo "=== Setup complete ==="
