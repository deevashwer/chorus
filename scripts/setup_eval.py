#!/usr/bin/env python3
"""Provision and set up the compute VM from the control VM.

Run this on the control VM.  It uses gcloud (pre-installed on all GCP
VMs) to create a large Ubuntu 22.04 compute VM and run the same setup
script.

Usage:
    python3 ~/chorus/scripts/setup_eval.py

Idempotent: safe to run multiple times.  Already-completed steps are
detected and skipped automatically.
"""

import json
import os
import subprocess
import sys
import time
import urllib.request

COMPUTE_VM_NAME = "chorus-compute"
MACHINE_TYPE = "c2d-standard-112"
BOOT_DISK_SIZE = "200GB"
IMAGE_FAMILY = "ubuntu-2204-lts"
IMAGE_PROJECT = "ubuntu-os-cloud"

REPO_DIR = os.path.expanduser("~/chorus")


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



def run(cmd, *, check=True, capture=False):
    print(f"    $ {' '.join(cmd)}")
    if capture:
        r = subprocess.run(cmd, capture_output=True, text=True)
        if check and r.returncode != 0:
            print(r.stderr, file=sys.stderr)
            sys.exit(r.returncode)
        return r
    return subprocess.run(cmd, check=check)


def vm_exists(project: str, zone: str) -> bool:
    r = run(["gcloud", "compute", "instances", "describe", COMPUTE_VM_NAME,
             "--project", project, "--zone", zone, "--format=json"],
            check=False, capture=True)
    if r.returncode != 0:
        return False
    info = json.loads(r.stdout)
    status = info.get("status", "")
    print(f"    Compute VM already exists (status: {status})")
    return status == "RUNNING"


def create_vm(project: str, zone: str):
    print(f"    Creating VM '{COMPUTE_VM_NAME}' ({MACHINE_TYPE}, Ubuntu 22.04)...")
    run(["gcloud", "compute", "instances", "create", COMPUTE_VM_NAME,
         "--project", project,
         "--zone", zone,
         "--machine-type", MACHINE_TYPE,
         "--boot-disk-size", BOOT_DISK_SIZE,
         "--image-family", IMAGE_FAMILY,
         "--image-project", IMAGE_PROJECT])


def wait_for_ssh(project: str, zone: str, retries: int = 30, delay: int = 10):
    print("\n    Waiting for the compute VM to accept SSH connections...")
    for i in range(retries):
        r = run(["gcloud", "compute", "ssh", COMPUTE_VM_NAME,
                 "--project", project, "--zone", zone, "--",
                 "echo ok"],
                check=False, capture=True)
        if r.returncode == 0:
            print("    SSH is ready.")
            return
        print(f"    Attempt {i + 1}/{retries} — not ready yet, retrying in {delay}s...")
        time.sleep(delay)
    sys.exit("    Timed out waiting for SSH on the compute VM.")


def ssh_cmd(project: str, zone: str, command: str):
    run(["gcloud", "compute", "ssh", COMPUTE_VM_NAME,
         "--project", project, "--zone", zone, "--",
         f"bash -lc '{command}'"])


def copy_repo(project: str, zone: str):
    """Copy the repo from this control VM to the compute VM."""
    run(["gcloud", "compute", "scp", "--recurse",
         REPO_DIR,
         f"{COMPUTE_VM_NAME}:~/",
         "--project", project, "--zone", zone])


def provision(project: str, zone: str):
    print("\n  Step 1/2: Copying Chorus repository to the compute VM...")
    copy_repo(project, zone)
    print("  Step 1/2: done.")

    print("\n  Step 2/2: Running setup script (Rust, deps, build, generate)...")
    ssh_cmd(project, zone, "cd ~/chorus && bash scripts/setup.sh")
    print("  Step 2/2: done.")


def main():
    print()
    print("=" * 62)
    print("  Chorus Evaluation — Compute VM Setup")
    print("=" * 62)
    print()
    print("  This script creates a large Ubuntu 22.04 compute VM")
    print("  and runs the same setup script to install deps and")
    print("  build the artifact.")
    print()
    print("  All steps are idempotent — you can re-run this safely.")
    print()

    print("  Detecting GCP project and zone from instance metadata...")
    project = gcp_project()
    zone = gcp_zone()
    print(f"    Project: {project}")
    print(f"    Zone:    {zone}")
    print()

    print("-" * 62)
    print("  Phase 1: Ensure the compute VM exists")
    print("-" * 62)

    if vm_exists(project, zone):
        print(f"    Skipping creation — '{COMPUTE_VM_NAME}' is already running.")
    else:
        create_vm(project, zone)
        wait_for_ssh(project, zone)

    print()
    print("-" * 62)
    print("  Phase 2: Install dependencies and build the artifact")
    print("-" * 62)

    provision(project, zone)

    print()
    print("=" * 62)
    print("  Setup complete!  The compute VM is ready.")
    print()
    print("  Next step — run experiments:")
    print("    python3 ~/chorus/scripts/run_experiment.py")
    print()
    print("  When you are finished with all experiments, tear down")
    print("  the compute VM to stop billing:")
    print("    python3 ~/chorus/scripts/teardown.py")
    print("=" * 62)
    print()


if __name__ == "__main__":
    main()
