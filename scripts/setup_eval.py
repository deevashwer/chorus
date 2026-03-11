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

REPO_DIR = os.path.expanduser("~/chorus")
CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "config.json")

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)

_vm = CONFIG["compute_vm"]
COMPUTE_VM_NAME = _vm["name"]
MACHINE_TYPE = _vm["machine_type"]
BOOT_DISK_SIZE = _vm["boot_disk_size"]
IMAGE_FAMILY = _vm["image_family"]
IMAGE_PROJECT = _vm["image_project"]


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


def gcp_network() -> str:
    """Return the network name of the current VM's first NIC."""
    raw = metadata("instance/network-interfaces/0/network")
    # raw looks like "projects/<project>/global/networks/<name>"
    return raw.rsplit("/", 1)[-1]


def gcp_subnet() -> str | None:
    """Return the subnet URI of the current VM's first NIC, or None."""
    try:
        return metadata("instance/network-interfaces/0/subnetwork")
    except Exception:
        return None



def run(cmd, *, check=True, capture=False):
    print(f"    $ {' '.join(cmd)}")
    if capture:
        r = subprocess.run(cmd, capture_output=True, text=True)
        if check and r.returncode != 0:
            print(r.stderr, file=sys.stderr)
            sys.exit(r.returncode)
        return r
    return subprocess.run(cmd, check=check)


def fmt_elapsed(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    if h:
        return f"{h}h {m}m {s}s"
    if m:
        return f"{m}m {s}s"
    return f"{s}s"


def timed(label: str):
    """Context manager that prints wall-clock time for a block."""
    class _Timer:
        def __enter__(self):
            self.t0 = time.time()
            return self
        def __exit__(self, *_):
            elapsed = time.time() - self.t0
            print(f"\n  ⏱  {label}: {fmt_elapsed(elapsed)}")
    return _Timer()


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


def create_vm(project: str, zone: str, network: str, subnet: str | None):
    print(f"    Creating VM '{COMPUTE_VM_NAME}' ({MACHINE_TYPE}, Ubuntu 22.04)...")
    print(f"    Using network '{network}', subnet '{subnet or '(auto)'}'")
    cmd = ["gcloud", "compute", "instances", "create", COMPUTE_VM_NAME,
           "--project", project,
           "--zone", zone,
           "--machine-type", MACHINE_TYPE,
           "--boot-disk-size", BOOT_DISK_SIZE,
           "--image-family", IMAGE_FAMILY,
           "--image-project", IMAGE_PROJECT,
           "--network", network]
    if subnet:
        cmd += ["--subnet", subnet]
    run(cmd)


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
    """Rsync the working tree to the compute VM, excluding .gitignore'd files."""
    gitignore = os.path.join(REPO_DIR, ".gitignore")
    cmd = [
        "gcloud", "compute", "scp", "--recurse",
        "--project", project, "--zone", zone,
    ]
    # Build a tar locally, excluding patterns from .gitignore and .git/
    # Then scp + extract, so we don't need rsync on both sides.
    exclude_args = ["--exclude=.git"]
    if os.path.isfile(gitignore):
        with open(gitignore) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    # Strip trailing slashes — tar --exclude doesn't match with them
                    exclude_args.append(f"--exclude={line.rstrip('/')}")
    archive = "/tmp/chorus-repo.tar.gz"
    run(["tar", "czf", archive] + exclude_args + ["-C", os.path.dirname(REPO_DIR),
         os.path.basename(REPO_DIR)])
    # Copy the tarball to the compute VM
    run(["gcloud", "compute", "scp", archive,
         f"{COMPUTE_VM_NAME}:/tmp/chorus-repo.tar.gz",
         "--project", project, "--zone", zone])
    # Extract on the compute VM (strip the top-level directory name)
    ssh_cmd(project, zone,
            "mkdir -p ~/chorus && tar xzf /tmp/chorus-repo.tar.gz --strip-components=1 -C ~/chorus && rm /tmp/chorus-repo.tar.gz")


def provision(project: str, zone: str):
    with timed("Copy repo to compute VM"):
        copy_repo(project, zone)

    with timed("Install system packages & Rust"):
        ssh_cmd(project, zone, "cd ~/chorus && bash scripts/setup_deps.sh")

    with timed("Build (cargo build --release)"):
        ssh_cmd(project, zone, "cd ~/chorus && python3 scripts/run.py build")

    with timed("Generate benchmark state"):
        ssh_cmd(project, zone, "cd ~/chorus && python3 scripts/run.py generate")


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

    print("  Detecting GCP project, zone, and network from instance metadata...")
    project = gcp_project()
    zone = gcp_zone()
    network = gcp_network()
    subnet = gcp_subnet()
    print(f"    Project: {project}")
    print(f"    Zone:    {zone}")
    print(f"    Network: {network}")
    print(f"    Subnet:  {subnet or '(auto)'}")
    print()

    overall_t0 = time.time()

    print("-" * 62)
    print("  Phase 1: Ensure the compute VM exists")
    print("-" * 62)

    with timed("Phase 1 — VM creation"):
        if vm_exists(project, zone):
            print(f"    Skipping creation — '{COMPUTE_VM_NAME}' is already running.")
        else:
            create_vm(project, zone, network, subnet)
            wait_for_ssh(project, zone)

    print()
    print("-" * 62)
    print("  Phase 2: Install dependencies and build the artifact")
    print("-" * 62)

    with timed("Phase 2 — provision (copy, deps, build, generate)"):
        provision(project, zone)

    overall_elapsed = time.time() - overall_t0
    print()
    print("=" * 62)
    print(f"  Setup complete!  Total wall time: {fmt_elapsed(overall_elapsed)}")
    print()
    print("  The compute VM is ready.")
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
