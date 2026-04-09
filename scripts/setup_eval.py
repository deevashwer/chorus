#!/usr/bin/env python3
"""Set up the Chorus evaluation environment.

Run this on the control VM.  On first run it configures the compute VM
connection (GCP auto-detect or manual entry), then copies the repo,
installs dependencies, builds the artifact, and generates benchmark
state.

Usage:
    python3 ~/chorus/scripts/setup_eval.py

Idempotent: safe to run multiple times.  Already-completed steps are
detected and skipped automatically.
"""

import json
import subprocess
import sys
import time
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent

sys.path.insert(0, str(REPO_DIR / "scripts"))
from ssh_utils import load_vm_config, ssh_cmd, scp_to, wait_for_ssh, VM_CONFIG_PATH
from configure_vms import configure_gcp, configure_manual, _is_on_gcp

CONFIG_PATH = REPO_DIR / "config.json"
with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)


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
    class _Timer:
        def __enter__(self):
            self.t0 = time.time()
            return self
        def __exit__(self, *_):
            elapsed = time.time() - self.t0
            print(f"\n  [time] {label}: {fmt_elapsed(elapsed)}")
    return _Timer()


# ---------------------------------------------------------------------------
# Configuration -- runs on first invocation if vm_config.json is missing
# ---------------------------------------------------------------------------

def ensure_configured():
    """If vm_config.json already exists, offer to keep it. Otherwise
    auto-detect GCP or ask for manual details."""
    if VM_CONFIG_PATH.exists():
        try:
            existing = json.loads(VM_CONFIG_PATH.read_text())
            c = existing.get("compute", {})
            mode = existing.get("mode", "unknown")
            print(f"  Found existing compute VM config (mode: {mode}):")
            print(f"    Host: {c.get('host', '?')}")
            print(f"    User: {c.get('user', '?')}")
            print(f"    Key:  {c.get('key', '(default)')}")
            print()
            answer = input("  Keep these settings? [Y/n]: ").strip().lower()
            if answer in ("", "y", "yes"):
                return
        except (json.JSONDecodeError, KeyError):
            pass
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

        if choice == "1":
            configure_gcp()
        else:
            configure_manual()
    else:
        print("  This machine is not a GCP VM -- using manual configuration.")
        configure_manual()


# ---------------------------------------------------------------------------
# Control VM (local) setup
# ---------------------------------------------------------------------------

def setup_control_vm():
    """Install deps and build on the control VM (this machine)."""
    print("  Installing system packages & Rust on control VM...")
    run(["bash", str(REPO_DIR / "scripts" / "setup_deps.sh")])

    # Source cargo env and build
    print("  Building on control VM...")
    run(["bash", "-lc", f"cd {REPO_DIR} && python3 scripts/run.py build"])


# ---------------------------------------------------------------------------
# Compute VM (remote) provisioning
# ---------------------------------------------------------------------------

def copy_repo(cfg: dict):
    """Rsync the working tree to the compute VM, excluding .gitignore'd files."""
    gitignore = REPO_DIR / ".gitignore"
    exclude_args = ["--exclude=.git"]
    if gitignore.is_file():
        with open(gitignore) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    exclude_args.append(f"--exclude={line.rstrip('/')}")

    archive = "/tmp/chorus-repo.tar.gz"
    tar_cmd = ["tar", "czf", archive] + exclude_args
    if sys.platform == "darwin":
        tar_cmd += ["--no-mac-metadata", "--no-xattrs"]
    tar_cmd += ["-C", str(REPO_DIR.parent), REPO_DIR.name]
    run(tar_cmd)
    scp_to(cfg, archive, "/tmp/chorus-repo.tar.gz")
    ssh_cmd(cfg,
            "mkdir -p ~/chorus && "
            "tar xzf /tmp/chorus-repo.tar.gz --strip-components=1 -C ~/chorus && "
            "rm /tmp/chorus-repo.tar.gz")


SAVE_STATE_LOG = "/tmp/chorus_save_state.log"
LOG_CANARY = "CHORUS_BENCHMARK_OK"


def save_state_complete(cfg: dict) -> bool:
    r = ssh_cmd(cfg, f"grep -q {LOG_CANARY} {SAVE_STATE_LOG} 2>/dev/null",
                check=False, capture=True)
    return r.returncode == 0


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print()
    print("=" * 62)
    print("  Chorus Evaluation -- Setup")
    print("=" * 62)
    print()
    print("  This script sets up both the control VM (this machine)")
    print("  and the compute VM (remote).  All steps are idempotent.")
    print()

    overall_t0 = time.time()

    # Phase 1: configure the compute VM connection
    print("-" * 62)
    print("  Phase 1: Configure compute VM connection")
    print("-" * 62)
    print()
    ensure_configured()

    # Phase 2: verify connectivity
    vm_cfg = load_vm_config()
    cfg = vm_cfg["compute"]
    print()
    print("-" * 62)
    print(f"  Phase 2: Verify SSH to {cfg['user']}@{cfg['host']}")
    print("-" * 62)
    wait_for_ssh(cfg, retries=6, delay=5)

    # Phase 3: install deps & build on both VMs in parallel
    print()
    print("-" * 62)
    print("  Phase 3: Install deps & build on both VMs (parallel)")
    print("-" * 62)
    print()

    control_log = REPO_DIR / "control_setup.log"
    print(f"  Starting control VM setup in background (log: {control_log})")
    control_proc = subprocess.Popen(
        ["bash", "-lc",
         f"cd {REPO_DIR} && bash scripts/setup_deps.sh && "
         f"python3 scripts/run.py build"],
        stdout=open(control_log, "w"),
        stderr=subprocess.STDOUT,
    )

    with timed("Compute VM (copy, deps, build)"):
        copy_repo(cfg)
        ssh_cmd(cfg, "cd ~/chorus && bash scripts/setup_deps.sh")
        ssh_cmd(cfg, "cd ~/chorus && python3 scripts/run.py build")

    print("\n  Waiting for control VM setup to finish...")
    rc = control_proc.wait()
    if rc != 0:
        sys.exit(f"  Control VM setup failed (exit {rc}). See {control_log}")
    print("  Control VM setup done.")

    # Phase 4: generate benchmark state on compute VM
    print()
    print("-" * 62)
    print("  Phase 4: Generate benchmark state on compute VM")
    print("-" * 62)

    if save_state_complete(cfg):
        print("\n    SAVE_STATE already completed (canary found) -- skipping.")
    else:
        with timed("Generate benchmark state (~3 h)"):
            ssh_cmd(cfg,
                    f"cd ~/chorus && python3 scripts/run.py generate "
                    f"2>&1 | tee {SAVE_STATE_LOG}")

    overall_elapsed = time.time() - overall_t0
    print()
    print("=" * 62)
    print(f"  Setup complete!  Total wall time: {fmt_elapsed(overall_elapsed)}")
    print()
    print("  Both VMs are ready.  Next step -- run experiments:")
    print("    python3 ~/chorus/scripts/run_experiment.py")
    print("=" * 62)
    print()


if __name__ == "__main__":
    main()
