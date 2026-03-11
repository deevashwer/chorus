#!/usr/bin/env python3
"""Log in to the Chorus evaluation control VM.

Run this on your local machine.  It connects via SSH and opens a
persistent GNU screen session.  Each local machine gets its own session,
so you can disconnect at any time and re-run this script later to resume
exactly where you left off — running experiments continue in the
background.

Usage:
    python3 scripts/login.py
"""

import json
import os
import platform
import subprocess
import sys
from pathlib import Path

CONFIG_FILE = Path.home() / ".chorus-eval.json"


def banner():
    print()
    print("=" * 62)
    print("  Chorus Artifact Evaluation — Login")
    print("=" * 62)
    print()
    print("  This script connects you to the evaluation control VM and")
    print("  opens a persistent terminal session (GNU screen).")
    print()
    print("  If you disconnect (or close your laptop), any running")
    print("  experiment keeps going.  Just re-run this script to")
    print("  reconnect and see the output.")
    print()


def ask(prompt, default=None):
    if default:
        val = input(f"  {prompt} [{default}]: ").strip()
        return val or default
    while True:
        val = input(f"  {prompt}: ").strip()
        if val:
            return val
        print("    (this field is required)")


def resolve_key(path_str):
    p = Path(path_str).expanduser().resolve()
    if not p.exists():
        sys.exit(f"\n  Error: file not found: {p}")
    return str(p)


def load_config():
    if CONFIG_FILE.exists():
        try:
            return json.loads(CONFIG_FILE.read_text())
        except (json.JSONDecodeError, OSError):
            pass
    return None


def save_config(cfg):
    CONFIG_FILE.write_text(json.dumps(cfg, indent=2))
    CONFIG_FILE.chmod(0o600)


def gather_config():
    """Interactively gather connection details."""
    print("-" * 62)
    print()
    print("  The authors should have given you:")
    print("    1. An SSH private key file")
    print("    2. The matching public key file")
    print("    3. The control VM's IP address")
    print()

    host = ask("Control VM IP address")
    user = ask("SSH user", default="ubuntu")
    print()

    print("  Now provide the SSH key pair you received from the authors.")
    print()
    priv = ask("Path to SSH private key (e.g. ~/chorus_eval_key)")
    priv = resolve_key(priv)

    pub = ask("Path to SSH public key  (e.g. ~/chorus_eval_key.pub)")
    pub = resolve_key(pub)

    return {"host": host, "user": user, "private_key": priv, "public_key": pub}


def confirm_config(cfg):
    print()
    print("-" * 62)
    print()
    print("  Connection details:")
    print(f"    Host:        {cfg['host']}")
    print(f"    User:        {cfg['user']}")
    print(f"    Private key: {cfg['private_key']}")
    print(f"    Public key:  {cfg['public_key']}")
    print()


def connect(cfg):
    hostname = platform.node() or "evaluator"
    session = f"chorus-{hostname}"

    print("-" * 62)
    print()
    print(f"  Connecting to {cfg['user']}@{cfg['host']}...")
    print(f"  Screen session: {session}")
    print()
    print("  Once inside, run these on the control VM:")
    print()
    print("    python3 ~/chorus/scripts/setup_eval.py     # first-time setup")
    print("    python3 ~/chorus/scripts/run_experiment.py  # run experiments")
    print()
    print("  To detach without stopping anything: Ctrl-A, then D")
    print("  To scroll up in screen:              Ctrl-A, then Esc")
    print()
    print("=" * 62)
    print()

    os.execvp("ssh", [
        "ssh",
        "-t",
        "-i", cfg["private_key"],
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        f"{cfg['user']}@{cfg['host']}",
        f"screen -dRR {session}",
    ])


def main():
    banner()

    saved = load_config()

    if saved:
        print("  Found saved connection details from a previous run.")
        confirm_config(saved)
        answer = input("  Use these settings? [Y/n]: ").strip().lower()
        if answer in ("", "y", "yes"):
            cfg = saved
            # Re-verify keys still exist
            for key in ("private_key", "public_key"):
                if not Path(cfg[key]).exists():
                    print(f"\n  Warning: {key} no longer exists at {cfg[key]}")
                    print("  Let's re-enter the connection details.\n")
                    cfg = gather_config()
                    break
        else:
            cfg = gather_config()
    else:
        cfg = gather_config()

    confirm_config(cfg)
    save_config(cfg)
    print(f"  (Connection details saved to {CONFIG_FILE})")
    print()

    connect(cfg)


if __name__ == "__main__":
    main()
