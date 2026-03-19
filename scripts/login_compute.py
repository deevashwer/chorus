#!/usr/bin/env python3
"""SSH into the compute VM from the control VM.

Usage:
    python3 ~/chorus/scripts/login_compute.py
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ssh_utils import load_vm_config


def main():
    vm_cfg = load_vm_config()
    cfg = vm_cfg["compute"]

    print()
    print(f"  Connecting to compute VM ({cfg['user']}@{cfg['host']})...")
    print()

    ssh_args = ["ssh"]
    if cfg.get("key"):
        ssh_args += ["-i", cfg["key"]]
    ssh_args += [
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        f"{cfg['user']}@{cfg['host']}",
    ]
    os.execvp("ssh", ssh_args)


if __name__ == "__main__":
    main()
