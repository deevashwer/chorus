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
    - Logs saved to results/<experiment>/
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

REPO_DIR = Path(__file__).resolve().parent.parent
RESULTS_DIR = REPO_DIR / "results"
LOCK_FILE = RESULTS_DIR / "lock.json"
TIMINGS_FILE = RESULTS_DIR / "timings.json"

CONFIG_PATH = REPO_DIR / "config.json"

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)

COMPUTE_VM_NAME = CONFIG["compute_vm"]["name"]


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
# Helpers — timing
# ---------------------------------------------------------------------------

def fmt_elapsed(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    if h:
        return f"{h}h {m}m {s}s"
    if m:
        return f"{m}m {s}s"
    return f"{s}s"


class timed:
    """Context manager that prints wall-clock time for a block."""
    def __init__(self, label: str):
        self.label = label
        self.elapsed = 0.0
    def __enter__(self):
        self.t0 = time.time()
        return self
    def __exit__(self, *_):
        self.elapsed = time.time() - self.t0
        print(f"\n  ⏱  {self.label}: {fmt_elapsed(self.elapsed)}")


# ---------------------------------------------------------------------------
# Helpers — running commands
# ---------------------------------------------------------------------------

def run_local(cmd, *, cwd=None, log_path=None, env_extra=None):
    """Run a command locally, streaming output and optionally saving to a log."""
    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)
    print(f"    $ {' '.join(str(c) for c in cmd)}")
    if log_path:
        with open(log_path, "w") as log_fh:
            proc = subprocess.Popen(
                cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, cwd=cwd or REPO_DIR, env=env,
            )
            for line in proc.stdout:
                sys.stdout.write("    " + line)
                log_fh.write(line)
            proc.wait()
        if proc.returncode != 0:
            raise RuntimeError(f"Command failed (exit {proc.returncode}): {' '.join(str(c) for c in cmd)}")
    else:
        subprocess.run(cmd, cwd=cwd or REPO_DIR, env=env, check=True)


def ssh_cmd(project, zone, command, *, log_path=None):
    """Run a command on the compute VM via SSH, streaming output."""
    cmd = [
        "gcloud", "compute", "ssh", COMPUTE_VM_NAME,
        "--project", project, "--zone", zone, "--",
        f"bash -lc '{command}'",
    ]
    run_local(cmd, log_path=log_path)


def scp_from_compute(project, zone, remote_path, local_path):
    """Copy a file from the compute VM to the local machine."""
    subprocess.run([
        "gcloud", "compute", "scp",
        f"{COMPUTE_VM_NAME}:{remote_path}", str(local_path),
        "--project", project, "--zone", zone,
    ], check=True)


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
# Existing-results detection (log files only — plots are cheap to regenerate)
# ---------------------------------------------------------------------------

def _expected_logs(exp_id: str) -> list[str]:
    """Return the list of log filenames an experiment produces."""
    exp_cfg = CONFIG.get("experiments", {}).get(exp_id, {})
    return [step["log"] for step in exp_cfg.get("steps", [])]


def find_existing_logs(exp_id: str) -> Path | None:
    """Return the most-recent run directory that contains all expected log
    files, or None if no complete set of logs exists."""
    exp_base = RESULTS_DIR / exp_id
    if not exp_base.is_dir():
        return None
    expected = _expected_logs(exp_id)
    if not expected:
        return None
    # Iterate timestamped subdirectories, newest first
    subdirs = sorted(
        (d for d in exp_base.iterdir() if d.is_dir()),
        key=lambda d: d.name,
        reverse=True,
    )
    for run_dir in subdirs:
        if all((run_dir / name).exists() for name in expected):
            return run_dir
    return None


def prompt_rerun(exp: dict) -> str:
    """Check for existing log files and ask what to do.

    Returns:
        'run'   — no prior logs, run from scratch
        'reuse' — user chose to reuse existing logs (just re-plot)
        'rerun' — user chose to re-run benchmarks
        'skip'  — user cancelled
    """
    prev_dir = find_existing_logs(exp["id"])
    if prev_dir is None:
        return "run"

    expected = _expected_logs(exp["id"])

    print()
    print(f"  📁  Experiment '{exp['id']}' already has log files:")
    print(f"      {prev_dir}")
    print()
    for log_name in expected:
        p = prev_dir / log_name
        size = p.stat().st_size
        print(f"        {log_name:40s}  ({size:,} bytes)")
    print()
    print("  Options:")
    print("    [u] Use existing logs — skip benchmarks, just re-plot")
    print("    [r] Re-run benchmarks from scratch")
    print("    [s] Skip — do nothing")
    print()
    try:
        answer = input("  Choice [u/r/s]: ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        return "skip"

    if answer in ("u", "use"):
        return "reuse"
    elif answer in ("r", "rerun"):
        return "rerun"
    else:
        return "skip"


# ---------------------------------------------------------------------------
# Build / sync helpers
# ---------------------------------------------------------------------------

def sync_to_compute(project: str, zone: str):
    """Sync the local repo to the compute VM and rebuild.

    Uses the same tar-based approach as setup_eval.py (respects .gitignore,
    excludes .git/).  Then rebuilds so the compute VM has up-to-date
    scripts and binaries.
    """
    gitignore = REPO_DIR / ".gitignore"
    exclude_args = ["--exclude=.git"]
    if gitignore.is_file():
        for line in gitignore.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                exclude_args.append(f"--exclude={line.rstrip('/')}")

    archive = "/tmp/chorus-repo.tar.gz"
    print("    Creating archive of local repo...")
    run_local(
        ["tar", "czf", archive] + exclude_args +
        ["-C", str(REPO_DIR.parent), REPO_DIR.name],
    )
    print("    Copying to compute VM...")
    run_local([
        "gcloud", "compute", "scp", archive,
        f"{COMPUTE_VM_NAME}:/tmp/chorus-repo.tar.gz",
        "--project", project, "--zone", zone,
    ])
    print("    Extracting on compute VM...")
    ssh_cmd(project, zone,
            "mkdir -p ~/chorus && tar xzf /tmp/chorus-repo.tar.gz "
            "--strip-components=1 -C ~/chorus && rm /tmp/chorus-repo.tar.gz")
    print("    Rebuilding on compute VM...")
    ssh_cmd(project, zone, "cd ~/chorus && python3 scripts/run.py build")


def ensure_local_build():
    """Ensure the control VM has Rust and the project is built."""
    # Check if cargo is available
    r = subprocess.run(["bash", "-lc", "command -v cargo"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        print("    Rust not found on control VM — installing deps...")
        run_local(["bash", "scripts/setup_deps.sh"], cwd=REPO_DIR)

    print("    Building on control VM (if needed)...")
    run_local(
        ["bash", "-lc", "cd ~/chorus && python3 scripts/run.py build"],
        cwd=REPO_DIR,
    )


def ensure_matplotlib():
    """Ensure matplotlib and numpy are available for plotting."""
    try:
        import matplotlib  # noqa: F401
    except ImportError:
        print("    Installing matplotlib and numpy for plotting...")
        subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet",
             "matplotlib", "numpy"],
            check=True,
        )


# ---------------------------------------------------------------------------
# Experiment: Figure 5 — saVSS vs cgVSS
# ---------------------------------------------------------------------------

def run_figure5(project: str, zone: str, exp_dir: Path,
                reuse_from: Path | None = None):
    """Run Figure 5: saVSS vs cgVSS (NIVSS benchmarks).

    If *reuse_from* is set, benchmark logs are copied from that directory
    and only the plotting step is executed.

    Steps (when running from scratch):
      1. sa_nivss verify-dealing on compute VM  (server mode)
      2. sa_nivss + pv_nivss deal/receive on control VM  (client modes)
      3. Generate plots
    """
    import shutil

    exp_cfg = CONFIG.get("experiments", {}).get("figure5", {})
    steps = exp_cfg.get("steps", [])
    run_py = "python3 scripts/run.py"

    if reuse_from is not None:
        print()
        print(f"  Reusing logs from {reuse_from}")
        for step in steps:
            src = reuse_from / step["log"]
            dst = exp_dir / step["log"]
            shutil.copy2(src, dst)
            print(f"    ✓ {step['log']}")
    else:
        # Group steps by VM
        compute_steps = [s for s in steps if s["vm"] == "compute"]
        control_steps = [s for s in steps if s["vm"] == "control"]

        # --- Compute VM benchmarks ---
        if compute_steps:
            print()
            print("  Running benchmarks on compute VM ...")
            with timed("Compute VM benchmarks"):
                sync_to_compute(project, zone)
                for step in compute_steps:
                    bench = step["bench"]
                    mode = step.get("mode")
                    log_name = step["log"]
                    bench_cmd = f"{run_py} bench {bench}" + (f" {mode}" if mode else "")
                    remote_log = f"/tmp/{log_name}"
                    ssh_cmd(project, zone,
                            f"cd ~/chorus && {bench_cmd} 2>&1"
                            f" | tee {remote_log}")
                    scp_from_compute(project, zone, remote_log,
                                     exp_dir / log_name)

        # --- Control VM benchmarks ---
        if control_steps:
            print()
            print("  Running benchmarks on control VM ...")
            with timed("Control VM benchmarks"):
                ensure_local_build()
                for step in control_steps:
                    bench = step["bench"]
                    mode = step.get("mode")
                    log_name = step["log"]
                    log_path = exp_dir / log_name
                    bench_cmd = f"{run_py} bench {bench}" + (f" {mode}" if mode else "")
                    run_local(
                        ["bash", "-c",
                         f"cd {REPO_DIR} && {bench_cmd} 2>&1"
                         f" | tee {log_path}"],
                    )

    # Plot (always runs — cheap)
    print()
    print("  Generating plots ...")
    with timed("Plotting"):
        ensure_matplotlib()
        run_local(
            [sys.executable, str(REPO_DIR / "experiments" / "plot_nivss.py"),
             "--results-dir", str(exp_dir)],
        )

    # List generated files
    print()
    print("  Generated files:")
    for f in sorted(exp_dir.iterdir()):
        size = f.stat().st_size
        print(f"    {f.name:40s}  ({size:,} bytes)")


# ---------------------------------------------------------------------------
# Experiment registry
# ---------------------------------------------------------------------------

EXPERIMENTS = [
    {
        "id": "figure5",
        "description": "Figure 5: saVSS vs cgVSS runtime and communication",
        "expected_minutes": CONFIG.get("experiments", {})
                                  .get("figure5", {})
                                  .get("expected_minutes", 120),
        "run": run_figure5,
    },
]


# ---------------------------------------------------------------------------
# Interactive menu
# ---------------------------------------------------------------------------

def print_menu(timings: dict):
    print()
    print("  Available experiments:")
    print()
    for i, exp in enumerate(EXPERIMENTS, 1):
        dur = expected_duration_str(exp, timings)
        done = find_existing_logs(exp["id"])
        status = "  ✅ logs exist" if done else ""
        print(f"    {i}. [{exp['id']}]  {exp['description']}{status}")
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
    print("  This script runs benchmark experiments across the control")
    print("  and compute VMs and saves results to results/.")
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

    # Check whether log files already exist from a previous run
    action = prompt_rerun(exp)
    if action == "skip":
        print("  Skipping — nothing changed.")
        return

    reuse_from = None
    if action == "reuse":
        reuse_from = find_existing_logs(exp["id"])

    print()
    if reuse_from:
        print(f"  Re-plotting experiment: {exp['id']}  (reusing existing logs)")
    else:
        print(f"  Starting experiment: {exp['id']}")
    print(f"  {exp['description']}")

    acquire_lock(exp["id"], exp["expected_minutes"])

    def _cleanup(signum, frame):
        release_lock()
        sys.exit(128 + signum)

    signal.signal(signal.SIGINT, _cleanup)
    signal.signal(signal.SIGTERM, _cleanup)

    ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    exp_dir = RESULTS_DIR / exp["id"] / ts
    exp_dir.mkdir(parents=True, exist_ok=True)

    start = time.time()
    try:
        exp["run"](project, zone, exp_dir, reuse_from=reuse_from)
        success = True
    except Exception as e:
        print(f"\n  Experiment '{exp['id']}' FAILED: {e}")
        success = False
    finally:
        release_lock()

    duration = time.time() - start
    if success:
        save_timing(exp["id"], duration)

    print()
    print("=" * 62)
    if success:
        print(f"  Experiment '{exp['id']}' completed in {fmt_elapsed(duration)}.")
        print(f"  Results saved to: {exp_dir}")
    else:
        print(f"  Experiment '{exp['id']}' failed after {fmt_elapsed(duration)}.")
        print(f"  Check logs in: {exp_dir}")
    print()
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
