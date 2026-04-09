#!/usr/bin/env python3
"""Tear down the compute VM when done with evaluation.

- GCP mode:  deletes the compute VM via gcloud to stop billing.
- Manual mode:  prints a reminder (you manage your own VMs).

Run this on the control VM when you are done with all experiments.

Usage:
    python3 ~/chorus/scripts/teardown.py
"""

import json
import subprocess
import sys
import time
from pathlib import Path

VM_CONFIG_PATH = Path(__file__).resolve().parent.parent / "vm_config.json"


def load_vm_config():
    if not VM_CONFIG_PATH.exists():
        return None
    try:
        return json.loads(VM_CONFIG_PATH.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def teardown_gcp(vm_cfg):
    gcp = vm_cfg["gcp"]
    project = gcp["project"]
    zone = gcp["zone"]
    vm_name = gcp["vm_name"]

    print(f"  This will permanently delete the compute VM")
    print(f"  '{vm_name}' in {zone}.")
    print()

    answer = input("  Are you sure? [y/N]: ").strip().lower()
    if answer not in ("y", "yes"):
        print("  Cancelled.")
        return

    print()
    print(f"  Deleting '{vm_name}'...")
    t0 = time.time()
    r = subprocess.run([
        "gcloud", "compute", "instances", "delete", vm_name,
        "--project", project, "--zone", zone, "--quiet",
    ])
    elapsed = time.time() - t0

    if r.returncode == 0:
        print(f"  Done -- '{vm_name}' has been deleted.")
    else:
        print(f"  Deletion failed (exit code {r.returncode}).")
        sys.exit(r.returncode)

    m, s = divmod(int(elapsed), 60)
    print(f"\n  [time] Tear down: {m}m {s}s")


def teardown_manual():
    print("  You are using manually-provisioned VMs.")
    print("  When you are done with the evaluation, remember to")
    print("  shut down or delete both VMs:")
    print()
    print("    - Control VM  (this machine)")
    print("    - Compute VM  (the remote machine)")
    print()
    print("  If you used a cloud provider, delete the instances")
    print("  from their console or CLI to stop billing.")


def main():
    print()
    print("=" * 62)
    print("  Chorus Evaluation -- Tear Down")
    print("=" * 62)
    print()

    vm_cfg = load_vm_config()
    if vm_cfg is None:
        print("  No vm_config.json found -- nothing to tear down.")
        return

    mode = vm_cfg.get("mode", "manual")
    if mode == "gcp" and "gcp" in vm_cfg:
        teardown_gcp(vm_cfg)
    else:
        teardown_manual()

    print()
    print("=" * 62)
    print()


if __name__ == "__main__":
    main()
