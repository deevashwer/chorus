#!/usr/bin/env python3
"""Interactive experiment runner for Chorus artifact evaluation.

Run this on the control VM after setup_eval.py has completed.
Best used inside a screen/tmux session — that way long-running
experiments survive disconnections.

Usage:
    python3 ~/chorus/scripts/run_experiment.py
    python3 ~/chorus/scripts/run_experiment.py all
    python3 ~/chorus/scripts/run_experiment.py <experiment_id>

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
import shlex
import argparse
import signal
import subprocess
import sys
import time
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
RESULTS_DIR = REPO_DIR / "results"
LOCK_FILE = RESULTS_DIR / "lock.json"
TIMINGS_FILE = RESULTS_DIR / "timings.json"

CONFIG_PATH = REPO_DIR / "config.json"

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)

sys.path.insert(0, str(REPO_DIR / "scripts"))
from ssh_utils import (
    load_vm_config,
    ssh_cmd as _ssh_remote,
    ssh_cmd_raw as _ssh_raw,
    scp_to as _scp_to,
    scp_from as _scp_from,
    get_remote_interface,
)


def _load_compute_cfg() -> dict:
    """Load and return the compute VM SSH config dict."""
    vm_cfg = load_vm_config()
    return vm_cfg["compute"]


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


def ssh_cmd(cfg, command, *, log_path=None):
    """Run a command on the compute VM via SSH, streaming output."""
    from ssh_utils import _ssh_base, _target
    cmd = _ssh_base(cfg) + [_target(cfg), f"bash -lc {shlex.quote(command)}"]
    run_local(cmd, log_path=log_path)


def scp_from_compute(cfg, remote_path, local_path):
    """Copy a file from the compute VM to the local machine."""
    _scp_from(cfg, remote_path, str(local_path))


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
    if not LOCK_FILE.exists():
        return None
    try:
        lock = json.loads(LOCK_FILE.read_text())
    except (json.JSONDecodeError, OSError):
        LOCK_FILE.unlink(missing_ok=True)
        return None

    if "pid" not in lock or pid_alive(lock["pid"]):
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

_EXP_CONFIG_KEY = {
    "table9":       "secret_recovery",
    "table6":       "secret_recovery",
    "table7":       "secret_recovery",
    "figure8":      "secret_recovery",
    "appendixA41":  "secret_recovery",
    "server_cost":  "secret_recovery",
}


_SR_NEEDS_CLIENT = {
    "table6": True,
    "table7": True,
    "figure8": True,
    "table9": False,
    "appendixA41": True,
    "server_cost": False,
}


def _expected_logs(exp_id: str) -> list[str]:
    config_key = _EXP_CONFIG_KEY.get(exp_id, exp_id)
    if config_key not in CONFIG["experiments"]:
        return []
    steps = CONFIG["experiments"][config_key]["steps"]
    if exp_id in _SR_NEEDS_CLIENT and not _SR_NEEDS_CLIENT[exp_id]:
        return [s["log"] for s in steps if s["vm"] == "compute"]
    return [s["log"] for s in steps]


def _log_base_dir(exp_id: str) -> str:
    return _EXP_CONFIG_KEY.get(exp_id, exp_id)


LOG_CANARY = "CHORUS_BENCHMARK_OK"


def _log_is_complete(path: Path) -> bool:
    try:
        text = path.read_text(errors="replace")
        return LOG_CANARY in text
    except OSError:
        return False


def find_existing_logs(exp_id: str) -> Path | None:
    expected = _expected_logs(exp_id)
    if not expected:
        return None

    log_base = RESULTS_DIR / _log_base_dir(exp_id)
    if not log_base.is_dir():
        return None

    subdirs = sorted(
        (d for d in log_base.iterdir() if d.is_dir()),
        key=lambda d: d.name,
        reverse=True,
    )
    for run_dir in subdirs:
        logs = [run_dir / name for name in expected]
        if all(p.exists() and _log_is_complete(p) for p in logs):
            return run_dir
    return None


def prompt_rerun(exp: dict) -> str:
    prev_dir = find_existing_logs(exp["id"])
    if prev_dir is None:
        return "run"

    expected = _expected_logs(exp["id"])

    print()
    print(f"  Experiment '{exp['id']}' already has log files:")
    print(f"      {prev_dir}")
    print()
    for log_name in expected:
        p = prev_dir / log_name
        size = p.stat().st_size
        print(f"        {log_name:40s}  ({size:,} bytes)")
    print()
    print("  Options:")
    print("    [u] Use existing logs — skip benchmarks, just re-generate")
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

def sync_to_compute(cfg: dict):
    """Sync the local repo to the compute VM and rebuild."""
    gitignore = REPO_DIR / ".gitignore"
    exclude_args = ["--exclude=.git"]
    if gitignore.is_file():
        for line in gitignore.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                exclude_args.append(f"--exclude={line.rstrip('/')}")

    archive = "/tmp/chorus-repo.tar.gz"
    print("    Creating archive of local repo...")
    tar_cmd = ["tar", "czf", archive] + exclude_args
    if sys.platform == "darwin":
        tar_cmd += ["--no-mac-metadata", "--no-xattrs"]
    tar_cmd += ["-C", str(REPO_DIR.parent), REPO_DIR.name]
    run_local(tar_cmd)
    print("    Copying to compute VM...")
    _scp_to(cfg, archive, "/tmp/chorus-repo.tar.gz")
    print("    Extracting on compute VM...")
    _ssh_remote(cfg,
                "mkdir -p ~/chorus && tar xzf /tmp/chorus-repo.tar.gz "
                "--strip-components=1 -C ~/chorus && rm /tmp/chorus-repo.tar.gz")
    print("    Rebuilding on compute VM...")
    _ssh_remote(cfg, "cd ~/chorus && python3 scripts/run.py build")


def ensure_local_build():
    """Ensure the control VM has Rust and the project is built."""
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
    try:
        import matplotlib  # noqa: F401
    except ImportError:
        print("    Installing matplotlib and numpy for plotting...")
        subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet",
             "matplotlib", "numpy"],
            check=True,
        )


def ensure_tqdm():
    try:
        import tqdm  # noqa: F401
    except ImportError:
        print("    Installing tqdm ...")
        subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet", "tqdm"],
            check=True,
        )


# ---------------------------------------------------------------------------
# Network limiting helpers
# ---------------------------------------------------------------------------

def get_default_interface():
    """Get the default network interface on the local (control) VM."""
    r = subprocess.run(
        ["bash", "-c", "ip -o -4 route show to default | head -1 | cut -d' ' -f5"],
        capture_output=True, text=True,
    )
    return r.stdout.strip() or "ens4"


def get_compute_ip(cfg: dict) -> str:
    """Return the compute VM's IP address (from vm_config.json)."""
    return cfg["host"]


def apply_network_limit(cfg: dict):
    """Apply bandwidth and latency limits on both VMs."""
    nl = CONFIG["network_limit"]
    bw = nl["bandwidth_mbps"]
    rtt = nl["rtt_ms"]
    delay = rtt // 2

    local_iface = get_default_interface()
    remote_iface = get_remote_interface(cfg)

    print()
    print("  " + "=" * 58)
    print(f"  ⚠  APPLYING NETWORK LIMITS between VMs")
    print(f"      Bandwidth : {bw} Mbps")
    print(f"      RTT       : {rtt} ms  ({delay} ms each direction)")
    print(f"      Control VM interface : {local_iface}")
    print(f"      Compute VM interface : {remote_iface}")
    print("  " + "=" * 58)

    # Control VM (local)
    subprocess.run(
        ["sudo", "tc", "qdisc", "del", "dev", local_iface, "root"],
        capture_output=True,
    )
    run_local(["sudo", "tc", "qdisc", "add", "dev", local_iface,
               "root", "handle", "1:", "netem", "delay", f"{delay}ms"])
    run_local(["sudo", "tc", "qdisc", "add", "dev", local_iface,
               "parent", "1:", "tbf", "rate", f"{bw}mbit",
               "burst", "32kbit", "latency", "400ms"])

    # Compute VM (remote)
    _ssh_remote(cfg,
                f"sudo tc qdisc del dev {remote_iface} root 2>/dev/null; "
                f"sudo tc qdisc add dev {remote_iface} root handle 1: "
                f"netem delay {delay}ms && "
                f"sudo tc qdisc add dev {remote_iface} parent 1: tbf "
                f"rate {bw}mbit burst 32kbit latency 400ms")

    print("      ✓ Network limits applied on both VMs.")
    print()


def remove_network_limit(cfg: dict):
    """Remove any tc qdiscs on both VMs (best-effort, never fails)."""
    local_iface = get_default_interface()
    subprocess.run(
        ["sudo", "tc", "qdisc", "del", "dev", local_iface, "root"],
        capture_output=True,
    )
    try:
        remote_iface = get_remote_interface(cfg)
        _ssh_remote(cfg,
                    f"sudo tc qdisc del dev {remote_iface} root 2>/dev/null || true",
                    check=False)
    except Exception:
        pass

    print()
    print("  " + "=" * 58)
    print("  ⚠  NETWORK LIMITS REMOVED on both VMs")
    print("  " + "=" * 58)
    print()


def start_server_on_compute(cfg: dict):
    """Start the Chorus network server on the compute VM in background."""
    bench_cases = ",".join(str(c["case"]) for c in CONFIG["bench_cases"])
    num_clients = ",".join(CONFIG["num_clients"])
    port = CONFIG["network"]["server_port"]

    print("    Building server binary on compute VM...")
    _ssh_remote(cfg,
                "cd ~/chorus && RUSTFLAGS='-A warnings' "
                "cargo build --release --bin server")

    # Kill any leftover server from a previous interrupted run
    _ssh_remote(cfg,
                f"kill $(cat /tmp/chorus_server.pid 2>/dev/null) 2>/dev/null; "
                f"fuser -k {port}/tcp 2>/dev/null; "
                f"rm -f /tmp/chorus_server.pid /tmp/chorus_server.log; "
                f"sleep 1; true",
                check=False)

    _ssh_remote(cfg,
                f"cd ~/chorus && "
                f"BENCH_CASES={bench_cases} NUM_CLIENTS={num_clients} "
                f"SERVER_PORT={port} "
                f"setsid ./target/release/server "
                f"> /tmp/chorus_server.log 2>&1 & echo $! > /tmp/chorus_server.pid && sleep 1")

    print("    Waiting for server to be ready (loading state, may take a few minutes)...")
    wait_cmd = (
        'timeout 300 bash -c \''
        'while ! grep -q "Server listening" /tmp/chorus_server.log 2>/dev/null; '
        "do sleep 2; done'"
    )
    r = _ssh_remote(cfg, wait_cmd, check=False, capture=True)
    if r.returncode != 0:
        print("    Server did not become ready within timeout. Server log:")
        diag = _ssh_remote(cfg,
                           "cat /tmp/chorus_server.log 2>/dev/null || echo '[no log file]'",
                           check=False, capture=True)
        for line in diag.stdout.strip().splitlines():
            print(f"      {line}")
        raise RuntimeError("Server failed to start — see log above.")
    print("    ✓ Server is listening.")


def stop_server_on_compute(cfg: dict):
    """Kill the Chorus network server on the compute VM (best-effort)."""
    try:
        port = CONFIG["network"]["server_port"]
        _ssh_remote(cfg,
                    f"kill $(cat /tmp/chorus_server.pid) 2>/dev/null; "
                    f"fuser -k {port}/tcp 2>/dev/null; "
                    f"rm -f /tmp/chorus_server.pid /tmp/chorus_server.log || true",
                    check=False)
        print("    ✓ Server stopped on compute VM.")
    except Exception:
        pass


def copy_state_dirs(cfg: dict):
    """Copy case_*_clients_* directories from compute VM to control VM."""
    print("    Archiving state directories on compute VM...")
    _ssh_remote(cfg,
                "cd ~/chorus && tar czf /tmp/chorus_case_dirs.tar.gz "
                "case_*_clients_* 2>/dev/null || true")
    print("    Copying archive to control VM...")
    scp_from_compute(cfg, "/tmp/chorus_case_dirs.tar.gz",
                     "/tmp/chorus_case_dirs.tar.gz")
    print("    Extracting on control VM...")
    run_local(["tar", "xzf", "/tmp/chorus_case_dirs.tar.gz",
               "-C", str(REPO_DIR)])
    run_local(["rm", "-f", "/tmp/chorus_case_dirs.tar.gz"])
    _ssh_remote(cfg, "rm -f /tmp/chorus_case_dirs.tar.gz")
    print("    ✓ State directories copied.")


# ---------------------------------------------------------------------------
# Experiment: Figure 5 — saVSS vs cgVSS
# ---------------------------------------------------------------------------

def run_figure5(cfg: dict, exp_dir: Path,
                reuse_from: Path | None = None):
    import shutil

    exp_cfg = CONFIG["experiments"]["figure5"]
    steps = exp_cfg["steps"]
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
        compute_steps = [s for s in steps if s["vm"] == "compute"]
        control_steps = [s for s in steps if s["vm"] == "control"]

        if compute_steps:
            print()
            print("  Running benchmarks on compute VM ...")
            with timed("Compute VM benchmarks"):
                sync_to_compute(cfg)
                for step in compute_steps:
                    bench = step["bench"]
                    mode = step["mode"]
                    log_name = step["log"]
                    bench_cmd = f"{run_py} bench {bench}" + (f" {mode}" if mode else "")
                    remote_log = f"/tmp/{log_name}"
                    ssh_cmd(cfg,
                            f"cd ~/chorus && {bench_cmd} 2>&1"
                            f" | tee {remote_log}")
                    scp_from_compute(cfg, remote_log, exp_dir / log_name)

        if control_steps:
            print()
            print("  Running benchmarks on control VM ...")
            with timed("Control VM benchmarks"):
                ensure_local_build()
                for step in control_steps:
                    bench = step["bench"]
                    mode = step["mode"]
                    log_name = step["log"]
                    log_path = exp_dir / log_name
                    bench_cmd = f"{run_py} bench {bench}" + (f" {mode}" if mode else "")
                    run_local(
                        ["bash", "-c",
                         f"cd {REPO_DIR} && {bench_cmd} 2>&1"
                         f" | tee {log_path}"],
                        env_extra={"CARGO_FEATURES": "client-parallel-bench"},
                    )

    print()
    print("  Generating plots ...")
    with timed("Plotting"):
        ensure_matplotlib()
        run_local(
            [sys.executable, str(REPO_DIR / "experiments" / "generate_figure5.py"),
             "--results-dir", str(exp_dir)],
        )

    print()
    print("  Generated files:")
    for f in sorted(exp_dir.iterdir()):
        size = f.stat().st_size
        print(f"    {f.name:40s}  ({size:,} bytes)")


# ---------------------------------------------------------------------------
# Experiment: Secret Recovery — split into server-only and client runners
# ---------------------------------------------------------------------------

def _run_sr_server_benchmark(cfg: dict, exp_dir: Path):
    run_py = "python3 scripts/run.py"
    print()
    print("  Running SERVER benchmark on compute VM ...")
    with timed("Server benchmark (compute VM)"):
        sync_to_compute(cfg)
        remote_log = "/tmp/secret_recovery_server.log"
        ssh_cmd(cfg,
                f"cd ~/chorus && "
                f"{run_py} bench secret_recovery server "
                f"2>&1 | tee {remote_log}",
                log_path=None)
        scp_from_compute(cfg, remote_log,
                         exp_dir / "secret_recovery_server.log")


def _run_sr_client_benchmark(cfg: dict, exp_dir: Path):
    run_py = "python3 scripts/run.py"
    server_started = False
    network_limited = False

    try:
        print()
        print("  Copying pre-generated state to control VM ...")
        with timed("Copy state directories"):
            copy_state_dirs(cfg)

        print()
        print("  Starting network server on compute VM ...")
        start_server_on_compute(cfg)
        server_started = True

        print()
        print("  Building on control VM (if needed) ...")
        ensure_local_build()

        compute_ip = get_compute_ip(cfg)
        print(f"    Compute VM IP: {compute_ip}")

        apply_network_limit(cfg)
        network_limited = True

        print()
        print("  Running CLIENT benchmark on control VM ...")
        print("  (Network is limited to "
              f"{CONFIG['network_limit']['bandwidth_mbps']} Mbps, "
              f"{CONFIG['network_limit']['rtt_ms']} ms RTT)")
        log_path = exp_dir / "secret_recovery_client.log"
        with timed("Client benchmark (control VM)"):
            run_local(
                ["bash", "-c",
                 f"cd {REPO_DIR} && {run_py} bench secret_recovery client "
                 f"2>&1 | tee {log_path}"],
                env_extra={
                    "WITH_NETWORK": "1",
                    "SERVER_IP": compute_ip,
                    "CARGO_FEATURES": "print-trace,client-parallel-bench",
                },
            )

    finally:
        if network_limited:
            remove_network_limit(cfg)
        if server_started:
            stop_server_on_compute(cfg)


_SR_SCRIPTS = {
    "table6":      "generate_table6.py",
    "table7":      "generate_table7.py",
    "figure8":     "generate_figure8.py",
    "table9":      "generate_table9.py",
    "appendixA41":  "generate_appendixA41.py",
    "server_cost":  "generate_server_cost.py",
}


def _find_or_create_sr_log_dir() -> Path:
    sr_base = RESULTS_DIR / "secret_recovery"
    if sr_base.is_dir():
        subdirs = sorted(
            (d for d in sr_base.iterdir() if d.is_dir()),
            key=lambda d: d.name,
            reverse=True,
        )
        if subdirs:
            return subdirs[0]
    ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    log_dir = sr_base / ts
    log_dir.mkdir(parents=True, exist_ok=True)
    return log_dir


def make_sr_runner(generate_target: str):
    needs_client = _SR_NEEDS_CLIENT[generate_target]

    def runner(cfg: dict, exp_dir: Path,
               reuse_from: Path | None = None):
        if reuse_from is not None:
            log_dir = reuse_from
            print()
            print(f"  Reusing logs from {log_dir}")
        else:
            log_dir = _find_or_create_sr_log_dir()

            server_log = log_dir / "secret_recovery_server.log"
            if not _log_is_complete(server_log):
                if server_log.exists():
                    print(f"  Server log incomplete (missing canary) — re-running.")
                    server_log.unlink()
                _run_sr_server_benchmark(cfg, log_dir)

            if needs_client:
                client_log = log_dir / "secret_recovery_client.log"
                if not _log_is_complete(client_log):
                    if client_log.exists():
                        print(f"  Client log incomplete (missing canary) — re-running.")
                        client_log.unlink()
                    _run_sr_client_benchmark(cfg, log_dir)

        script = _SR_SCRIPTS[generate_target]

        print()
        print(f"  Generating {generate_target} ...")
        with timed(f"Generating {generate_target}"):
            ensure_matplotlib()
            run_local(
                [sys.executable,
                 str(REPO_DIR / "experiments" / script),
                 "--results-dir", str(log_dir),
                 "--output-dir", str(exp_dir)],
            )

        print()
        print("  Generated files:")
        for f in sorted(exp_dir.iterdir()):
            size = f.stat().st_size
            print(f"    {f.name:40s}  ({size:,} bytes)")

    return runner


# ---------------------------------------------------------------------------
# Experiment: Table 10 — Parameter selection (pure computation, no benchmarks)
# ---------------------------------------------------------------------------

def run_table10(cfg: dict, exp_dir: Path,
                reuse_from: Path | None = None):
    print()
    print("  Generating Table 10 (parameter computation) ...")
    with timed("Generating table10"):
        run_local(
            [sys.executable,
             str(REPO_DIR / "experiments" / "generate_table10.py"),
             "--output-dir", str(exp_dir)],
        )

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
        "expected_minutes": CONFIG["experiments"]["figure5"]["expected_minutes"],
        "run": run_figure5,
    },
    {
        "id": "table9",
        "description": "Table 9: Server per-epoch costs (server benchmark only)",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["table9"],
        "run": make_sr_runner("table9"),
    },
    {
        "id": "table6",
        "description": "Table 6: Secret-recovery client costs",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["table6"],
        "run": make_sr_runner("table6"),
    },
    {
        "id": "table7",
        "description": "Table 7: Client committee costs and sortition frequency",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["table7"],
        "run": make_sr_runner("table7"),
    },
    {
        "id": "figure8",
        "description": "Figure 8: Client cost breakdown (time + communication)",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["figure8"],
        "run": make_sr_runner("figure8"),
    },
    {
        "id": "appendixA41",
        "description": "Appendix A.4.1: One-time DKG setup costs",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["appendixA41"],
        "run": make_sr_runner("appendixA41"),
    },
    {
        "id": "server_cost",
        "description": "Server dollar-cost estimation (server benchmark only)",
        "expected_minutes": CONFIG["experiments"]["secret_recovery"]["expected_minutes"]["server_cost"],
        "run": make_sr_runner("server_cost"),
    },
    {
        "id": "table10",
        "description": "Table 10: Parameter selection (n, threshold) vs. corruption/offline fractions",
        "expected_minutes": CONFIG["experiments"]["table10"]["expected_minutes"],
        "run": run_table10,
    },
]


# ---------------------------------------------------------------------------
# Interactive menu
# ---------------------------------------------------------------------------

def _run_all(cfg: dict, timings: dict, force: bool = False):
    if force:
        ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        fresh_sr = RESULTS_DIR / "secret_recovery" / ts
        fresh_sr.mkdir(parents=True, exist_ok=True)
        print(f"  --force: fresh benchmark logs will go to {fresh_sr}")
        print()

    total_start = time.time()
    results = []

    for exp in EXPERIMENTS:
        exp_id = exp["id"]
        print()
        print("=" * 62)
        print(f"  [{exp_id}]  {exp['description']}")
        print("=" * 62)

        reuse_from = None if force else find_existing_logs(exp_id)
        if reuse_from is not None:
            print(f"  Reusing existing logs from {reuse_from}")

        ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        exp_dir = RESULTS_DIR / exp_id / ts
        exp_dir.mkdir(parents=True, exist_ok=True)

        start = time.time()
        try:
            exp["run"](cfg, exp_dir, reuse_from=reuse_from)
            results.append((exp_id, True, time.time() - start))
            save_timing(exp_id, time.time() - start)
        except Exception as e:
            print(f"\n  Experiment '{exp_id}' FAILED: {e}")
            results.append((exp_id, False, time.time() - start))

    total_duration = time.time() - total_start
    print()
    print("=" * 62)
    print(f"  All experiments completed in {fmt_elapsed(total_duration)}.")
    print()
    for exp_id, ok, dur in results:
        status = "OK" if ok else "FAILED"
        print(f"    {exp_id:20s}  {status:6s}  {fmt_elapsed(dur)}")
    print("=" * 62)
    print()


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
    print(f"    a. Run ALL experiments sequentially")
    print(f"    0. Exit")
    print()


def _check_lock_or_exit():
    lock = check_lock()
    if not lock:
        return
    started = lock["started"]
    exp_name = lock["experiment"]
    expected = lock["expected_minutes"]
    elapsed = ""
    try:
        t0 = datetime.datetime.fromisoformat(started)
        mins = (datetime.datetime.now() - t0).total_seconds() / 60
        elapsed = f" ({mins:.0f} min elapsed so far)"
    except ValueError:
        pass
    print("-" * 62)
    print()
    print(f"  An experiment is already running (possibly started by")
    print(f"  another evaluator sharing this VM):")
    print()
    print(f"    Experiment:  {exp_name}")
    print(f"    Started at:  {started}{elapsed}")
    print(f"    Expected:    ~{expected} min total")
    print()
    print("  Please wait for it to finish, or check back later.")
    print()
    sys.exit(1)


def _run_single(cfg, exp, reuse_from):
    print()
    if reuse_from:
        print(f"  Re-generating experiment: {exp['id']}  (reusing existing logs)")
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
        exp["run"](cfg, exp_dir, reuse_from=reuse_from)
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
    print("=" * 62)
    print()


def main():
    valid_ids = [e["id"] for e in EXPERIMENTS]
    parser = argparse.ArgumentParser(
        description="Chorus Experiment Runner",
        epilog=f"Available experiment IDs: {', '.join(valid_ids)}",
    )
    parser.add_argument(
        "experiment", nargs="?", default=None,
        help="Experiment ID to run, or 'all' to run everything. "
             "Omit for interactive menu.",
    )
    parser.add_argument(
        "--force", action="store_true",
        help="Re-run all benchmarks from scratch (only with 'all').",
    )
    args = parser.parse_args()

    print()
    print("=" * 62)
    print("  Chorus Experiment Runner")
    print("=" * 62)
    print()

    cfg = _load_compute_cfg()
    print(f"  Compute VM: {cfg['user']}@{cfg['host']}")
    print()
    _check_lock_or_exit()

    # --- Non-interactive mode (CLI argument) ---
    if args.experiment is not None:
        selection = args.experiment.lower()
        if selection == "all":
            timings = load_timings()
            acquire_lock("all", sum(e["expected_minutes"] for e in EXPERIMENTS))
            try:
                _run_all(cfg, timings, force=args.force)
            finally:
                release_lock()
            return

        exp = next((e for e in EXPERIMENTS if e["id"] == selection), None)
        if exp is None:
            sys.exit(f"  Unknown experiment: {args.experiment}\n"
                     f"  Valid IDs: {', '.join(valid_ids)}")

        reuse_from = find_existing_logs(exp["id"])
        _run_single(cfg, exp, reuse_from)
        return

    # --- Interactive mode ---
    print("  This script runs benchmark experiments across the control")
    print("  and compute VMs and saves results to results/.")
    print()
    print("  Tip: run non-interactively with:")
    print("    python3 scripts/run_experiment.py all")
    print("    python3 scripts/run_experiment.py <experiment_id>")
    print()

    timings = load_timings()
    print_menu(timings)

    try:
        raw = input("  Select experiment number (or 'a' for all): ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        sys.exit("  Invalid selection.")

    if raw == "0":
        print("  Goodbye.")
        return

    if raw in ("a", "all"):
        acquire_lock("all", sum(e["expected_minutes"] for e in EXPERIMENTS))
        try:
            _run_all(cfg, timings)
        finally:
            release_lock()
        return

    try:
        choice = int(raw)
    except ValueError:
        sys.exit(f"  Invalid selection: {raw}")

    if choice < 1 or choice > len(EXPERIMENTS):
        sys.exit(f"  Invalid selection: {choice}")

    exp = EXPERIMENTS[choice - 1]

    action = prompt_rerun(exp)
    if action == "skip":
        print("  Skipping — nothing changed.")
        return

    reuse_from = None
    if action == "reuse":
        reuse_from = find_existing_logs(exp["id"])

    _run_single(cfg, exp, reuse_from)


if __name__ == "__main__":
    main()
