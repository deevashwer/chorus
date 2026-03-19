#!/usr/bin/env python3
"""Fetch experiment results from the control VM to your local machine.

Run this on your local machine (same place you run login.py).
It downloads the latest results for every experiment into a local
``results/`` directory.

Usage:
    python3 scripts/fetch_results.py              # fetch all
    python3 scripts/fetch_results.py table6 figure8  # fetch specific experiments
"""

import json
import os
import subprocess
import sys
from pathlib import Path

CONFIG_FILE = Path(__file__).resolve().parent.parent / "control_vm.json"
LOCAL_RESULTS = Path(__file__).resolve().parent.parent / "results"


def load_config():
    if not CONFIG_FILE.exists():
        sys.exit(
            f"No saved connection details found at {CONFIG_FILE}.\n"
            "Run  python3 scripts/login.py  first to set up the connection."
        )
    try:
        return json.loads(CONFIG_FILE.read_text())
    except (json.JSONDecodeError, OSError) as e:
        sys.exit(f"Error reading {CONFIG_FILE}: {e}")


def _ssh_target(cfg):
    user = cfg.get("user", "ubuntu")
    return f"{user}@{cfg['host']}"


def scp_recursive(cfg, remote_path, local_path):
    local_path.mkdir(parents=True, exist_ok=True)
    cmd = [
        "scp", "-r",
        "-i", cfg["key"],
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        f"{_ssh_target(cfg)}:{remote_path}",
        str(local_path),
    ]
    return subprocess.run(cmd, capture_output=True, text=True)


def list_remote_experiments(cfg):
    """List experiment directories available on the control VM."""
    cmd = [
        "ssh",
        "-i", cfg["key"],
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        _ssh_target(cfg),
        "ls -1 ~/chorus/results/ 2>/dev/null",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        return []
    return [d.strip() for d in r.stdout.strip().splitlines() if d.strip()]


def main():
    print()
    print("=" * 62)
    print("  Chorus — Fetch Results")
    print("=" * 62)
    print()

    cfg = load_config()
    print(f"  Control VM: {cfg['host']}")
    print()

    requested = set(sys.argv[1:]) if len(sys.argv) > 1 else None

    available = list_remote_experiments(cfg)
    if not available:
        sys.exit("  No results found on the control VM yet.")

    skip_dirs = {"timings.json"}
    experiments = [d for d in available if d not in skip_dirs]

    if requested:
        missing = requested - set(experiments)
        if missing:
            print(f"  Warning: not found on VM: {', '.join(sorted(missing))}")
        experiments = [e for e in experiments if e in requested]

    if not experiments:
        sys.exit("  Nothing to fetch.")

    print(f"  Fetching {len(experiments)} experiment(s):\n")

    for exp in sorted(experiments):
        remote = f"~/chorus/results/{exp}"
        local = LOCAL_RESULTS
        print(f"    {exp} ...", end=" ", flush=True)
        r = scp_recursive(cfg, remote, local)
        if r.returncode == 0:
            local_exp = local / exp
            count = sum(1 for _ in local_exp.rglob("*") if _.is_file())
            print(f"OK ({count} files)")
        else:
            print(f"FAILED")
            if r.stderr.strip():
                for line in r.stderr.strip().splitlines()[:3]:
                    print(f"      {line}")

    print()
    print(f"  Results saved to: {LOCAL_RESULTS}/")
    print()

    tex_files = sorted(LOCAL_RESULTS.rglob("*.tex"))
    png_files = sorted(LOCAL_RESULTS.rglob("*.png"))
    if tex_files or png_files:
        print("  Key output files:")
        for f in tex_files + png_files:
            print(f"    {f.relative_to(LOCAL_RESULTS)}")
        print()


if __name__ == "__main__":
    main()
