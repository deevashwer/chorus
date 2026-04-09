#!/usr/bin/env python3
"""Configure compute VM connection details for Chorus evaluation.

Run this on the control VM before running setup_eval.py.

Two modes:
  (A) GCP -- auto-detect project/zone, create the compute VM with gcloud,
      and populate vm_config.json automatically.
  (B) Manual -- you provide the compute VM's IP, SSH user, and key.

Usage:
    python3 ~/chorus/scripts/configure_vms.py
"""

import getpass
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
VM_CONFIG_PATH = REPO_DIR / "vm_config.json"
CONFIG_PATH = REPO_DIR / "config.json"

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)


def ask(prompt, default=None):
    if default:
        val = input(f"  {prompt} [{default}]: ").strip()
        return val or default
    while True:
        val = input(f"  {prompt}: ").strip()
        if val:
            return val
        print("    (this field is required)")


# -----------------------------------------------------------------------
# GCP helpers
# -----------------------------------------------------------------------

def _gcp_metadata(path: str) -> str:
    url = f"http://metadata.google.internal/computeMetadata/v1/{path}"
    req = urllib.request.Request(url, headers={"Metadata-Flavor": "Google"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        return resp.read().decode().strip()


def _is_on_gcp() -> bool:
    try:
        _gcp_metadata("project/project-id")
        return True
    except Exception:
        return False


def _gcp_project() -> str:
    return _gcp_metadata("project/project-id")


def _gcp_zone() -> str:
    return _gcp_metadata("instance/zone").rsplit("/", 1)[-1]


def _gcp_network() -> str:
    raw = _gcp_metadata("instance/network-interfaces/0/network")
    return raw.rsplit("/", 1)[-1]


def _gcp_subnet() -> str | None:
    try:
        return _gcp_metadata("instance/network-interfaces/0/subnetwork")
    except Exception:
        return None


def _gcp_vm_exists(project, zone, name) -> bool:
    r = subprocess.run(
        ["gcloud", "compute", "instances", "describe", name,
         "--project", project, "--zone", zone, "--format=json"],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        return False
    info = json.loads(r.stdout)
    return info.get("status") == "RUNNING"


def _gcp_create_vm(project, zone, network, subnet, vm_cfg):
    name = vm_cfg["name"]
    print(f"    Creating VM '{name}' ({vm_cfg['machine_type']}, Ubuntu 22.04)...")
    cmd = [
        "gcloud", "compute", "instances", "create", name,
        "--project", project,
        "--zone", zone,
        "--machine-type", vm_cfg["machine_type"],
        "--boot-disk-size", vm_cfg["boot_disk_size"],
        "--image-family", vm_cfg["image_family"],
        "--image-project", vm_cfg["image_project"],
        "--network", network,
    ]
    if subnet:
        cmd += ["--subnet", subnet]
    subprocess.run(cmd, check=True)


def _gcp_wait_for_ssh(project, zone, name, retries=30, delay=10):
    print(f"\n    Waiting for '{name}' to accept SSH connections...")
    for i in range(retries):
        r = subprocess.run(
            ["gcloud", "compute", "ssh", name,
             "--project", project, "--zone", zone, "--",
             "echo ok"],
            capture_output=True, text=True,
        )
        if r.returncode == 0:
            print("    SSH is ready.")
            return
        print(f"    Attempt {i+1}/{retries} -- not ready yet, retrying in {delay}s...")
        time.sleep(delay)
    sys.exit(f"    Timed out waiting for SSH on '{name}'.")


def _gcp_get_internal_ip(project, zone, name) -> str:
    r = subprocess.run(
        ["gcloud", "compute", "instances", "describe", name,
         "--project", project, "--zone", zone,
         "--format=get(networkInterfaces[0].networkIP)"],
        capture_output=True, text=True, check=True,
    )
    return r.stdout.strip()


def configure_gcp():
    """Auto-detect GCP environment, create compute VM, write vm_config.json."""
    print()
    print("  Detecting GCP project, zone, and network from instance metadata...")
    project = _gcp_project()
    zone = _gcp_zone()
    network = _gcp_network()
    subnet = _gcp_subnet()
    print(f"    Project: {project}")
    print(f"    Zone:    {zone}")
    print(f"    Network: {network}")
    print(f"    Subnet:  {subnet or '(auto)'}")
    print()

    vm_cfg = CONFIG["compute_vm"]
    vm_name = vm_cfg["name"]

    if _gcp_vm_exists(project, zone, vm_name):
        print(f"    Compute VM '{vm_name}' already exists and is RUNNING.")
    else:
        _gcp_create_vm(project, zone, network, subnet, vm_cfg)
        _gcp_wait_for_ssh(project, zone, vm_name)

    # Run gcloud ssh once to ensure keys are propagated
    print("    Ensuring SSH keys are set up...")
    subprocess.run(
        ["gcloud", "compute", "ssh", vm_name,
         "--project", project, "--zone", zone, "--",
         "echo ok"],
        capture_output=True, text=True, check=True,
    )

    internal_ip = _gcp_get_internal_ip(project, zone, vm_name)
    user = getpass.getuser()
    key_path = str(Path.home() / ".ssh" / "google_compute_engine")

    print(f"    Internal IP: {internal_ip}")
    print(f"    SSH user:    {user}")
    print(f"    SSH key:     {key_path}")

    config = {
        "mode": "gcp",
        "gcp": {
            "project": project,
            "zone": zone,
            "vm_name": vm_name,
        },
        "compute": {
            "host": internal_ip,
            "user": user,
            "key": key_path,
        },
    }
    VM_CONFIG_PATH.write_text(json.dumps(config, indent=2) + "\n")
    print()
    print(f"  Saved to {VM_CONFIG_PATH}")


GENERATED_KEY = Path.home() / ".ssh" / "chorus_compute_key"


def _ensure_key() -> str:
    """Return the path to a private key usable for the compute VM.

    If the key was already copied by login.py, use that.  Otherwise
    generate a new Ed25519 keypair and print the public key so the
    user can authorize it on the compute VM.
    """
    if GENERATED_KEY.exists():
        return str(GENERATED_KEY)

    print()
    print("  No compute VM key found on this machine.")
    print("  Options:")
    print()
    print("    [1] Generate a new keypair here (you'll paste the")
    print("        public key into the compute VM's authorized_keys)")
    print()
    print("    [2] Enter the path to a key already on this machine")
    print()
    choice = input("  Choice [1/2]: ").strip()

    if choice == "2":
        key = ask("Path to SSH private key on this machine")
        key_path = Path(key).expanduser().resolve()
        if not key_path.exists():
            sys.exit(f"\n  Error: key not found at {key_path}")
        return str(key_path)

    print(f"\n  Generating Ed25519 keypair at {GENERATED_KEY}...")
    GENERATED_KEY.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["ssh-keygen", "-t", "ed25519", "-f", str(GENERATED_KEY),
         "-N", "", "-C", "chorus-eval"],
        check=True,
    )
    pub = GENERATED_KEY.read_text().rstrip() if GENERATED_KEY.with_suffix(".pub").exists() \
        else "(could not read public key)"
    pub = GENERATED_KEY.with_suffix(".pub").read_text().rstrip()
    print()
    print("  Add this public key to the compute VM's ~/.ssh/authorized_keys:")
    print()
    print(f"    {pub}")
    print()
    input("  Press Enter once you've done that...")
    return str(GENERATED_KEY)


def configure_manual():
    """Interactively ask for compute VM connection details."""
    print()
    print("-" * 62)
    print("  Compute VM connection details")
    print("-" * 62)
    print()
    print("  You need a compute VM (Ubuntu 22.04, 112 vCPUs, 224 GB RAM, 200 GB disk)")
    print("  that this control VM can reach over the network.")
    print()

    defaults = {}
    if VM_CONFIG_PATH.exists():
        try:
            existing = json.loads(VM_CONFIG_PATH.read_text())
            defaults = existing.get("compute", {})
        except (json.JSONDecodeError, KeyError):
            pass

    host = ask("Compute VM IP address or hostname",
               defaults.get("host"))
    user = ask("SSH username on compute VM",
               defaults.get("user", "ubuntu"))

    key = defaults.get("key", "")
    if key and Path(key).exists():
        print(f"\n  Using existing key: {key}")
    else:
        key = _ensure_key()

    config = {
        "mode": "manual",
        "compute": {
            "host": host,
            "user": user,
            "key": key,
        },
    }

    VM_CONFIG_PATH.write_text(json.dumps(config, indent=2) + "\n")
    print()
    print(f"  Saved to {VM_CONFIG_PATH}")

    print()
    print("  Testing SSH connection...")
    ssh_cmd = ["ssh",
        "-i", key,
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        "-o", "ConnectTimeout=10",
        f"{user}@{host}",
        "echo ok",
    ]
    r = subprocess.run(ssh_cmd, capture_output=True, text=True)
    if r.returncode == 0:
        print("  SSH connection successful!")
    else:
        print("  SSH connection FAILED. Please verify:")
        print(f"    - The compute VM is running at {host}")
        print(f"    - User '{user}' can log in")
        print(f"    - Key '{key}' is authorized on the compute VM")
        print()
        print("  You can fix the settings and re-run this script.")
        sys.exit(1)


def main():
    """Standalone entry point -- reconfigure compute VM connection."""
    print()
    print("=" * 62)
    print("  Chorus Evaluation -- Reconfigure Compute VM")
    print("=" * 62)
    print()
    print("  Note: setup_eval.py runs this automatically on first use.")
    print("  Use this script to change the compute VM connection.")
    print()

    on_gcp = _is_on_gcp()

    if on_gcp:
        print("  Detected: this machine is a GCP VM.")
        print()
        print("  Choose how to set up the compute VM:")
        print()
        print("    [1] GCP (auto) -- create a compute VM in the same")
        print("        GCP project/zone using gcloud (recommended)")
        print()
        print("    [2] Manual -- provide your own compute VM's IP,")
        print("        SSH user, and key (any cloud or bare metal)")
        print()
        try:
            choice = input("  Choice [1/2]: ").strip()
        except (EOFError, KeyboardInterrupt):
            sys.exit("\n  Cancelled.")
    else:
        print("  This machine is not a GCP VM -- using manual configuration.")
        choice = "2"

    if choice == "1":
        configure_gcp()
    else:
        configure_manual()

    print()
    print("  Next step:")
    print("    python3 ~/chorus/scripts/setup_eval.py")
    print()


if __name__ == "__main__":
    main()
