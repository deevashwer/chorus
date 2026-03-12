#!/usr/bin/env bash
# Set up a fresh Ubuntu VM for Chorus benchmarks.
# Installs Rust, C/C++ dependencies, builds the artifact, and generates
# benchmark state.  Idempotent — safe to run multiple times.
#
# Usage:  cd ~/chorus && bash scripts/setup.sh
set -e

SETUP_START=$SECONDS

echo "=== Chorus VM Setup ==="

# System packages & Rust
bash scripts/setup_deps.sh
. "$HOME/.cargo/env"

# Build
python3 scripts/run.py build

# Generate benchmark state
python3 scripts/run.py generate

echo "=== Setup complete — total: $(( SECONDS - SETUP_START ))s ==="
