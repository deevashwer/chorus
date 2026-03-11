#!/usr/bin/env python3
"""Chorus artifact runner.

Usage:
    python3 scripts/run.py build
    python3 scripts/run.py generate
    python3 scripts/run.py bench server
    python3 scripts/run.py bench client
    python3 scripts/run.py serve

Environment variables (all optional):
    BENCH_CASES   Comma-separated case numbers        (default: 1,2)
    NUM_CLIENTS   Comma-separated client counts       (default: 1M,10M,100M)
    SERVER_IP     Server address for client benchmarks (default: 0.0.0.0)
    SERVER_PORT   TCP port for the network server      (default: 32000)
"""

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def run(cmd, *, cwd=None, env_extra=None, check=True):
    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)
    return subprocess.run(cmd, cwd=cwd or ROOT, env=env, check=check)


def cmd_build():
    print("=== Building Chorus artifact ===")
    run(["cargo", "build", "--release"], cwd=ROOT / "class_group")
    run(["cargo", "build", "--release"])
    print("\nBuild complete.  Binaries are in target/release/")


def cmd_generate():
    print("=== Generating benchmark state ===")
    run(["cargo", "bench", "--bench", "secret_recovery"],
        env_extra={"BENCHMARK_TYPE": "SAVE_STATE"})


def cmd_bench(mode):
    if mode not in ("server", "client"):
        sys.exit(f"Unknown bench mode '{mode}'.  Use 'server' or 'client'.")
    btype = mode.upper()
    print(f"=== Running {mode} benchmark ===")
    run(["cargo", "bench", "--bench", "secret_recovery"],
        env_extra={"BENCHMARK_TYPE": btype})


def cmd_serve():
    print("=== Starting network server ===")
    run([str(ROOT / "target" / "release" / "server")])


COMMANDS = {
    "build":    (cmd_build, "Compile both crates"),
    "generate": (cmd_generate, "Generate benchmark state (SAVE_STATE)"),
    "bench":    (None, "Run server or client benchmark"),
    "serve":    (cmd_serve, "Start the network server"),
}


def usage():
    print(__doc__)
    print("Commands:")
    for name, (_, desc) in COMMANDS.items():
        print(f"  {name:12s}  {desc}")
    sys.exit(1)


def main():
    if len(sys.argv) < 2:
        usage()
    cmd = sys.argv[1]
    if cmd in ("-h", "--help"):
        usage()
    if cmd == "bench":
        if len(sys.argv) < 3:
            sys.exit("Usage: run.py bench <server|client>")
        cmd_bench(sys.argv[2])
    elif cmd in COMMANDS:
        COMMANDS[cmd][0]()
    else:
        sys.exit(f"Unknown command '{cmd}'.  Run with --help for usage.")


if __name__ == "__main__":
    main()
