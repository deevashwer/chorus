#!/usr/bin/env bash
# Install system packages and Rust on a fresh Ubuntu VM.
# Idempotent — safe to run multiple times.
#
# Usage:  cd ~/chorus && bash scripts/setup_deps.sh
set -e

export LANG=C.UTF-8
export LC_ALL=C.UTF-8

echo "=== Installing system packages ==="
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    libgmp-dev libmpfr-dev libssl-dev m4 build-essential pkg-config \
    cmake python3 python3-pip curl

echo "=== Installing Rust ==="
if ! command -v cargo &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
. "$HOME/.cargo/env"

echo "=== Dependencies ready ==="
