#!/usr/bin/env python3
"""Interactive experiment runner for Chorus artifact evaluation.

Run this on the control VM after setup_eval.py has completed.
Best used inside the screen session started by login.py — that way
long-running experiments survive disconnections.

Usage:
    python3 ~/chorus/scripts/run_experiment.py

Features:
    - Interactive menu of experiments with expected durations
    - File-based locking prevents concurrent experiments
    - Timing database tracks past run durations
    - Logs saved to ~/results/<experiment>/
    - You can detach from screen (Ctrl-A D) while an experiment runs
      and reconnect later to see the output
"""

import datetime
import json
import os
import signal
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

RESULTS_DIR = Path.home() / "results"
LOCK_FILE = RESULTS_DIR / "lock.json"
TIMINGS_FILE = RESULTS_DIR / "timings.json"

COMPUTE_VM_NAME = "chorus-compute"

EXPERIMENTS = [
    {
        "id": "generate",
        "description": "Generate benchmark state (all cases, all client counts)",
        "command": "python3 scripts/run.py generate",
        "expected_minutes": 120,
    },
    {
        "id": "bench-server",
        "description": "Server benchmark (all cases, all client counts)",
        "command": "python3 scripts/run.py bench server",
        "expected_minutes": 180,
    },
    {
        "id": "bench-client",
        "description": "Client benchmark (all cases, all client counts)",
        "command": "python3 scripts/run.py bench client",
        "expected_minutes": 180,
    },
]


# ---------------------------------------------------------------------------
# GCP metadata
# ---------------------------------------------------------------------------

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


# ---------------------------------------------------------------------------
# Locking — prevents two evaluators from running experiments at the same time
# ---------------------------------------------------------------------------

def pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def check_lock():
    """Return lock info dict if a valid lock is held, else None."""
    if not LOCK_FILE.exists():
        return None
    try:
        lock = json.loads(LOCK_FILE.read_text())
    except (json.JSONDecodeError, OSError):
        LOCK_FILE.unlink(missing_ok=True)
        return None

    if pid_alive(lock.get("pid", -1)):
        return lock

    print("    (Cleaned up a stale lock from a previous crashed run.)")
    LOCK_FILE.unlink(missing_ok=True)
    return None


def acquire_lock(experiment_id: str, expected_minutes: int):
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    lock = {
        "experiment": experiment_id,
        "pid": os.getpid(),
        "started": datetime.datetime.now().isoformat(),
        "expected_minutes": expected_minutes,
    }
    LOCK_FILE.write_text(json.dumps(lock, indent=2))


def release_lock():
    LOCK_FILE.unlink(missing_ok=True)


# ---------------------------------------------------------------------------
# Timings — remembers how long past runs took
# ---------------------------------------------------------------------------

def load_timings() -> dict:
    if TIMINGS_FILE.exists():
        try:
            return json.loads(TIMINGS_FILE.read_text())
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def save_timing(experiment_id: str, duration_secs: float):
    timings = load_timings()
    timings.setdefault(experiment_id, []).append(round(duration_secs, 1))
    TIMINGS_FILE.write_text(json.dumps(timings, indent=2))


def expected_duration_str(exp: dict, timings: dict) -> str:
    past = timings.get(exp["id"], [])
    if past:
        mins = past[-1] / 60
        return f"~{mins:.0f} min (last run)"
    return f"~{exp['expected_minutes']} min (estimated)"


# ---------------------------------------------------------------------------
# Plot generation (stub — will be filled in later)
# ---------------------------------------------------------------------------

def generate_plots(experiment_id: str, log_path: Path):
    print(f"    Plot generation not yet implemented for '{experiment_id}'.")


# ---------------------------------------------------------------------------
# Experiment execution
# ---------------------------------------------------------------------------

def run_experiment(exp: dict, project: str, zone: str):
    exp_id = exp["id"]
    ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    exp_dir = RESULTS_DIR / exp_id
    exp_dir.mkdir(parents=True, exist_ok=True)
    log_path = exp_dir / f"{ts}.log"

    cmd = [
        "gcloud", "compute", "ssh", COMPUTE_VM_NAME,
        "--project", project, "--zone", zone, "--",
        f"bash -lc 'cd ~/chorus && {exp['command']}'",
    ]

    print()
    print(f"    Command:  {exp['command']}")
    print(f"    Log file: {log_path}")
    print()
    print("    Output will stream below.  You can safely detach from")
    print("    screen (Ctrl-A D) — the experiment keeps running.")
    print()
    print("    " + "-" * 54)

    start = time.time()
    with open(log_path, "w") as log_fh:
        proc = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
        )
        for line in proc.stdout:
            sys.stdout.write("    " + line)
            log_fh.write(line)
        proc.wait()
    print("    " + "-" * 54)

    duration = time.time() - start

    if proc.returncode != 0:
        print()
        print(f"    Experiment '{exp_id}' FAILED (exit code {proc.returncode}).")
        print(f"    Check the log for details: {log_path}")
        return False, duration

    save_timing(exp_id, duration)
    generate_plots(exp_id, log_path)

    print()
    print(f"    Experiment '{exp_id}' completed in {duration / 60:.1f} minutes.")
    print(f"    Log saved to: {log_path}")
    return True, duration


# ---------------------------------------------------------------------------
# Interactive menu
# ---------------------------------------------------------------------------

def print_menu(timings: dict):
    print()
    print("  Available experiments:")
    print()
    for i, exp in enumerate(EXPERIMENTS, 1):
        dur = expected_duration_str(exp, timings)
        print(f"    {i}. [{exp['id']}]  {exp['description']}")
        print(f"       Expected duration: {dur}")
    print()
    print("    0. Exit")
    print()


def main():
    print()
    print("=" * 62)
    print("  Chorus Experiment Runner")
    print("=" * 62)
    print()
    print("  This script runs a benchmark experiment on the compute VM")
    print("  and saves the results to ~/results/.")
    print()
    print("  You are inside a screen session, so you can safely detach")
    print("  (Ctrl-A D) while an experiment is running.  Re-run")
    print("  login.py on your local machine to reconnect and check")
    print("  progress.")
    print()

    project = gcp_project()
    zone = gcp_zone()

    # Check if another experiment is already running
    lock = check_lock()
    if lock:
        started = lock.get("started", "unknown")
        exp_name = lock.get("experiment", "unknown")
        expected = lock.get("expected_minutes", "?")
        elapsed = ""
        try:
            t0 = datetime.datetime.fromisoformat(started)
            mins = (datetime.datetime.now() - t0).total_seconds() / 60
            elapsed = f" ({mins:.0f} min elapsed so far)"
        except ValueError:
            pass
        print("-" * 62)
        print()
        print(f"  Another experiment is currently running:")
        print(f"    Experiment:  {exp_name}")
        print(f"    Started at:  {started}{elapsed}")
        print(f"    Expected:    ~{expected} min total")
        print()
        print("  Please wait for it to finish, or check back later.")
        print("  (If you're in the same screen session, scroll up to")
        print("  see its live output.)")
        print()
        sys.exit(1)

    timings = load_timings()
    print_menu(timings)

    try:
        choice = int(input("  Select experiment number: "))
    except (ValueError, EOFError):
        sys.exit("  Invalid selection.")

    if choice == 0:
        print("  Goodbye.")
        return

    if choice < 1 or choice > len(EXPERIMENTS):
        sys.exit(f"  Invalid selection: {choice}")

    exp = EXPERIMENTS[choice - 1]

    print()
    print(f"  Starting experiment: {exp['id']}")
    print(f"  {exp['description']}")

    acquire_lock(exp["id"], exp["expected_minutes"])

    def _cleanup(signum, frame):
        release_lock()
        sys.exit(128 + signum)

    signal.signal(signal.SIGINT, _cleanup)
    signal.signal(signal.SIGTERM, _cleanup)

    try:
        success, _ = run_experiment(exp, project, zone)
    finally:
        release_lock()

    print()
    print("=" * 62)
    print("  What to do next:")
    print()
    print("  • Run another experiment:")
    print("      python3 ~/chorus/scripts/run_experiment.py")
    print()
    print("  • When done with ALL experiments, tear down the compute VM:")
    print("      python3 ~/chorus/scripts/teardown.py")
    print("=" * 62)
    print()


if __name__ == "__main__":
    main()
