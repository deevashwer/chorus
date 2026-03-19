#!/usr/bin/env python3
"""Log in to the Chorus evaluation control VM.

Run this on your local machine.  It syncs the repo to the control VM,
installs screen, then connects via SSH and opens a persistent GNU
screen session.

Each local machine gets its own screen session, so you can disconnect
at any time and re-run this script later to resume exactly where you
left off — running experiments continue in the background.

Usage:
    python3 scripts/login.py
"""

import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
CONFIG_FILE = REPO_DIR / "control_vm.json"


def banner():
    print()
    print("=" * 62)
    print("  Chorus Artifact Evaluation — Login")
    print("=" * 62)
    print()
    print("  This script syncs the repo to the control VM and opens")
    print("  a persistent terminal session (GNU screen).")
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
    print("-" * 62)
    print()
    print("  You need:")
    print("    1. The control VM's IP address")
    print("    2. An SSH username and private key for the control VM")
    print()

    host = ask("Control VM IP address")
    print()

    user = ask("SSH username on control VM", "ubuntu")
    print()

    key = ask("Path to SSH private key (e.g. ~/chorus_eval_key)")
    key = resolve_key(key)

    pub = key + ".pub"
    if not Path(pub).exists():
        print(f"\n  Warning: expected public key at {pub} but not found.")
        print("  (SSH only needs the private key, continuing anyway.)")

    cfg = {"host": host, "user": user, "key": key}

    print()
    print("-" * 62)
    print()
    print("  If you have your own compute VM, enter its details now")
    print("  so the SSH key can be copied to the control VM.")
    print("  (Skip this if using the author-provided GCP setup.)")
    print()
    compute_host = input("  Compute VM IP address (Enter to skip): ").strip()

    if compute_host:
        print()
        compute_user = ask("SSH username on compute VM", "ubuntu")
        print()
        compute_key = ask("Path to SSH private key for compute VM")
        compute_key = resolve_key(compute_key)
        cfg["compute"] = {
            "host": compute_host,
            "user": compute_user,
            "key": compute_key,
        }

    return cfg


def confirm_config(cfg):
    print()
    print("-" * 62)
    print()
    print("  Control VM:")
    print(f"    Host: {cfg['host']}")
    print(f"    User: {cfg.get('user', 'ubuntu')}")
    print(f"    Key:  {cfg['key']}")
    if cfg.get("compute"):
        c = cfg["compute"]
        print()
        print("  Compute VM:")
        print(f"    Host: {c['host']}")
        print(f"    User: {c.get('user', 'ubuntu')}")
        print(f"    Key:  {c['key']}")
    print()


def _ssh_opts(cfg):
    """Common SSH/SCP options."""
    return [
        "-i", cfg["key"],
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
    ]


def _target(cfg):
    return f"{cfg.get('user', 'ubuntu')}@{cfg['host']}"


def ensure_prerequisites(cfg):
    """Ensure python3, screen, and curl are installed on the control VM."""
    print("  Ensuring prerequisites on control VM (python3, screen, curl)...")
    subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg),
         "sudo apt-get update -qq && "
         "sudo apt-get install -y --no-install-recommends "
         "screen python3 curl"],
        check=True,
    )
    print()


def repo_exists_on_remote(cfg) -> bool:
    """Check if ~/chorus already exists on the control VM."""
    r = subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg), "test -d ~/chorus"],
        capture_output=True,
    )
    return r.returncode == 0


def sync_repo(cfg):
    """Tar the local repo and extract it on the control VM."""
    print("  Syncing repo to control VM...")

    gitignore = REPO_DIR / ".gitignore"
    exclude_args = ["--exclude=.git"]
    if gitignore.is_file():
        for line in gitignore.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                exclude_args.append(f"--exclude={line.rstrip('/')}")

    archive = "/tmp/chorus-repo.tar.gz"
    tar_env = os.environ.copy()
    tar_env["COPYFILE_DISABLE"] = "1"
    tar_cmd = ["tar", "czf", archive] + exclude_args
    if sys.platform == "darwin":
        tar_cmd += ["--no-mac-metadata", "--no-xattrs"]
    tar_cmd += ["-C", str(REPO_DIR.parent), REPO_DIR.name]
    subprocess.run(tar_cmd, check=True, env=tar_env)

    subprocess.run(
        ["scp"] + _ssh_opts(cfg) + [archive, f"{_target(cfg)}:/tmp/chorus-repo.tar.gz"],
        check=True,
    )

    subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg),
         "mkdir -p ~/chorus && "
         "tar xzf /tmp/chorus-repo.tar.gz --strip-components=1 -C ~/chorus && "
         "rm /tmp/chorus-repo.tar.gz"],
        check=True,
    )

    os.unlink(archive)
    print("  Done.")
    print()


def provision_compute_config(cfg):
    """Copy compute VM SSH key to the control VM and write vm_config.json."""
    compute = cfg["compute"]
    remote_key_rel = ".ssh/chorus_compute_key"

    print("  Copying compute VM SSH key to control VM...")
    subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg), "mkdir -p ~/.ssh"],
        check=True,
    )
    subprocess.run(
        ["scp"] + _ssh_opts(cfg) + [
            compute["key"],
            f"{_target(cfg)}:{remote_key_rel}"],
        check=True,
    )
    subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg),
         f"chmod 600 ~/{remote_key_rel}"],
        check=True,
    )

    r = subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg),
         f"realpath ~/{remote_key_rel}"],
        capture_output=True, text=True, check=True,
    )
    abs_key_path = r.stdout.strip()

    vm_config = json.dumps({
        "mode": "manual",
        "compute": {
            "host": compute["host"],
            "user": compute["user"],
            "key": abs_key_path,
        },
    }, indent=2)
    subprocess.run(
        ["ssh"] + _ssh_opts(cfg) + [_target(cfg),
         f"cat > ~/chorus/vm_config.json << 'EOF'\n{vm_config}\nEOF"],
        check=True,
    )
    print("  Compute VM key and config written on control VM.")
    print()


def connect(cfg):
    hostname = platform.node() or "evaluator"
    tag = hashlib.sha256(hostname.encode()).hexdigest()[:12]
    session = f"chorus-{tag}"

    print("-" * 62)
    print()
    print(f"  Connecting to {_target(cfg)}...")
    print(f"  Screen session: {session}")
    print()
    print("  Once inside, run these on the control VM:")
    print()
    print("    python3 ~/chorus/scripts/setup_eval.py      # first-time setup")
    print("    python3 ~/chorus/scripts/run_experiment.py   # run experiments")
    print()
    print("  To detach without stopping anything: Ctrl-A, then D")
    print("  To scroll up in screen:              Ctrl-A, then Esc")
    print()
    print("=" * 62)
    input("\n  Press Enter to connect...")
    print()

    os.execvp("ssh", [
        "ssh", "-t",
    ] + _ssh_opts(cfg) + [
        _target(cfg),
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
            if not Path(cfg["key"]).exists():
                print(f"\n  Warning: key no longer exists at {cfg['key']}")
                print("  Let's re-enter the connection details.\n")
                cfg = gather_config()
        else:
            cfg = gather_config()
    else:
        cfg = gather_config()

    confirm_config(cfg)
    save_config(cfg)
    print(f"  (Connection details saved to {CONFIG_FILE})")
    print()

    ensure_prerequisites(cfg)

    if repo_exists_on_remote(cfg):
        print("  Repo already exists on control VM — skipping sync.")
        print("  (To force a re-sync, delete ~/chorus on the control VM.)")
        print()
    else:
        sync_repo(cfg)

    if cfg.get("compute"):
        provision_compute_config(cfg)

    connect(cfg)


if __name__ == "__main__":
    main()
