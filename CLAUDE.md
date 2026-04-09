# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Chorus is a research cryptography implementation for IEEE S&P 2026: "Secret Recovery with Ephemeral Client Committees." It implements secret recovery protocols using distributed key generation, non-interactive verifiable secret sharing (NIVSS), and identity-based encryption.

## Build & Test Commands

```bash
# Build (release mode required for crypto performance)
cargo build --release

# Build via helper script (reads config.json, sets RUSTFLAGS)
python3 scripts/run.py build

# Run unit tests
cargo test --release

# Run a specific benchmark
cargo bench --bench pv_nivss
cargo bench --bench sa_nivss
cargo bench --bench secret_recovery
cargo bench --bench serialization

# Run benchmarks via helper (server/client side)
python3 scripts/run.py bench secret_recovery server
python3 scripts/run.py bench secret_recovery client
python3 scripts/run.py bench sa_nivss server
python3 scripts/run.py bench pv_nivss

# Generate benchmark state (slow, ~3.5 hours)
python3 scripts/run.py generate

# Start network server for client-server benchmarks
python3 scripts/run.py serve
```

**System dependencies:** libgmp-dev, libmpfr-dev, libssl-dev, m4, cmake, pkg-config, build-essential (install via `scripts/setup_deps.sh`).

## Architecture

### Workspace Crates

- **chorus** (main crate, `/src`) — Core library with crypto modules, networking, and binaries
- **class-group** (`/class_group/`) — Class group cryptography (CL-HSM encryption), built via CMake with C++ bindings
- **gmp-mpfr-sys** (upstream crate, v1.6.4+) — GMP/MPFR FFI bindings; Android cross-compilation patch in `patches/`

### Core Module Layout (`/src`)

- `lib.rs` — Entry point, global constants (`COMMITTEE_SIZE=200`, `NUM_CLIENTS=5000`), stat-tracking macros
- `network.rs` — TCP client-server communication (tokio async, port 32000)
- `bin/server.rs`, `bin/client.rs` — Binary targets for distributed benchmarks
- `crypto/` — All cryptographic primitives:
  - `nivss/` — PV-NIVSS and SA-NIVSS (the paper's core contribution)
  - `ibe/` — Identity-Based Encryption (Boneh-Franklin)
  - `avd/` — Authenticated Verification Dictionary (sparse Merkle tree)
  - `proofs/` — Zero-knowledge proofs (Schnorr, DLEQ, CL-KoE, MSM)
  - `shamir/` — Shamir secret sharing
  - `sortition/` — VRF-based committee sortition
- `secret_recovery/` — End-to-end protocol: `server.rs` (distributed keygen), `client.rs` (recovery), `common.rs` (shared types/params)

### Key Dependencies

- **Arkworks** (ark-*) — Algebraic crypto on BLS12-377 / BW6-761 curves for SNARKs
- **rug / gmp-mpfr-sys** — Arbitrary-precision arithmetic for class groups
- **tokio** — Async networking runtime
- **criterion** — Benchmarking framework

### Feature Flags

- `parallel` — Enable rayon parallelism (default on)
- `deterministic` — Deterministic randomness for reproducible benchmarks (default on)
- `client-parallel` / `client-parallel-bench` — Parallelism modes for client-side class group operations

### Artifact Evaluation

Experiments are orchestrated across two VMs (compute + control) via Python scripts in `scripts/`. Configuration lives in `config.json` (benchmark cases, network emulation params, experiment definitions). Results are collected in `results/` and plotted via `experiments/*.py`.
