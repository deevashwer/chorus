#!/usr/bin/env python3
"""Tear down the compute VM to stop billing.

Run this on the control VM when you are done with all experiments.

Usage:
    python3 ~/chorus/scripts/teardown.py
"""

import subprocess
import sys
import urllib.request

COMPUTE_VM_NAME = "chorus-compute"


def metadata(path: str) -> str:
    url = f"http://metadata.google.internal/computeMetadata/v1/{path}"
    req = urllib.request.Request(url, headers={"Metadata-Flavor": "Google"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        return resp.read().decode().strip()


def gcp_project() -> str:
    return metadata("project/project-id")


def gcp_zone() -> str:
    full = metadata("instance/zone")
    return full.rsplit("/", 1)[-1]


def main():
    print()
    print("=" * 62)
    print("  Chorus Evaluation — Tear Down Compute VM")
    print("=" * 62)
    print()

    project = gcp_project()
    zone = gcp_zone()

    print(f"  This will permanently delete the compute VM")
    print(f"  '{COMPUTE_VM_NAME}' in {zone}.")
    print()

    answer = input("  Are you sure? [y/N]: ").strip().lower()
    if answer not in ("y", "yes"):
        print("  Cancelled.")
        return

    print()
    print(f"  Deleting '{COMPUTE_VM_NAME}'...")
    r = subprocess.run([
        "gcloud", "compute", "instances", "delete", COMPUTE_VM_NAME,
        "--project", project, "--zone", zone, "--quiet",
    ])

    if r.returncode == 0:
        print(f"  Done — '{COMPUTE_VM_NAME}' has been deleted.")
    else:
        print(f"  Deletion failed (exit code {r.returncode}).")
        sys.exit(r.returncode)

    print()


if __name__ == "__main__":
    main()
