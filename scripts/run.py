#!/usr/bin/env python3
"""Chorus artifact runner.

Usage:
    python3 scripts/run.py build
    python3 scripts/run.py generate
    python3 scripts/run.py bench server|client
    python3 scripts/run.py bench sa_nivss server|client
    python3 scripts/run.py bench pv_nivss
    python3 scripts/run.py serve

Configuration is read from config.json at the repository root.
Environment variables BENCH_CASES, NUM_CLIENTS, SERVER_BIND_IP,
and SERVER_PORT override the corresponding config.json values when set.
SERVER_IP (the IP clients connect to) is set by run_experiment.py.
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONFIG_PATH = ROOT / "config.json"

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)


def _env_defaults() -> dict:
    """Build env overrides from config.json for the benchmark binary.

    Environment variables already set in the process take precedence
    over values from config.json (BENCH_CASES, NUM_CLIENTS,
    SERVER_BIND_IP, SERVER_PORT, WITH_NETWORK).

    SERVER_IP (the address clients connect to) is NOT set here; it is
    injected by run_experiment.py after auto-detecting the compute VM's
    internal IP.
    """
    env = {}
    env["RUSTFLAGS"] = "-A warnings"
    cases = [str(c["case"]) for c in CONFIG["bench_cases"]]
    env["BENCH_CASES"] = os.environ.get("BENCH_CASES", ",".join(cases))
    env["NUM_CLIENTS"] = os.environ.get("NUM_CLIENTS", ",".join(CONFIG["num_clients"]))
    net = CONFIG["network"]
    env["SERVER_BIND_IP"] = os.environ.get("SERVER_BIND_IP", net["server_bind_ip"])
    env["SERVER_PORT"] = os.environ.get("SERVER_PORT", str(net["server_port"]))
    if "SERVER_IP" in os.environ:
        env["SERVER_IP"] = os.environ["SERVER_IP"]
    if "WITH_NETWORK" in os.environ:
        env["WITH_NETWORK"] = os.environ["WITH_NETWORK"]
    return env


def fmt_elapsed(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    if h:
        return f"{h}h {m}m {s}s"
    if m:
        return f"{m}m {s}s"
    return f"{s}s"


def run(cmd, *, cwd=None, env_extra=None, check=True):
    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)
    return subprocess.run(cmd, cwd=cwd or ROOT, env=env, check=check)


def cmd_build():
    print("=== Building Chorus artifact ===")
    t0 = time.time()
    env = {"RUSTFLAGS": "-A warnings"}
    run(["cargo", "build", "--release"], cwd=ROOT / "class_group", env_extra=env)
    run(["cargo", "build", "--release"], env_extra=env)
    print(f"\n[time] Build: {fmt_elapsed(time.time() - t0)}")


def cmd_generate():
    print("=== Generating benchmark state ===")
    t0 = time.time()
    env = _env_defaults()
    env["BENCHMARK_TYPE"] = "SAVE_STATE"
    run(["cargo", "bench", "--bench", "secret_recovery"], env_extra=env)
    print(f"\n[time] Generate: {fmt_elapsed(time.time() - t0)}")


def cmd_bench(bench_name, mode=None):
    """Run a benchmark.

    bench_name: 'secret_recovery', 'sa_nivss', or 'pv_nivss'
    mode:       'server' or 'client' (required for secret_recovery and sa_nivss)
    """
    needs_mode = {"secret_recovery", "sa_nivss"}
    valid_benches = {"secret_recovery", "sa_nivss", "pv_nivss"}

    if bench_name not in valid_benches:
        sys.exit(f"Unknown bench '{bench_name}'. Use one of: {', '.join(sorted(valid_benches))}")

    if bench_name in needs_mode:
        if mode not in ("server", "client"):
            sys.exit(f"'{bench_name}' requires a mode: server or client")
    elif mode is not None:
        sys.exit(f"'{bench_name}' does not accept a mode argument.")

    label = bench_name + (f" {mode}" if mode else "")
    print(f"=== Running {label} benchmark ===")
    t0 = time.time()
    env = _env_defaults()
    if mode:
        env["BENCHMARK_TYPE"] = mode.upper()
    cmd = ["cargo", "bench", "--bench", bench_name]
    features = os.environ.get("CARGO_FEATURES")
    if features:
        cmd += ["--features", features]
    run(cmd, env_extra=env)
    print(f"\n[time] Bench {label}: {fmt_elapsed(time.time() - t0)}")


def cmd_serve():
    print("=== Starting network server ===")
    env = _env_defaults()
    run([str(ROOT / "target" / "release" / "server")], env_extra=env)


COMMANDS = {
    "build":    (cmd_build, "Compile both crates"),
    "generate": (cmd_generate, "Generate benchmark state (SAVE_STATE)"),
    "bench":    (None, "Run a benchmark  (see below)"),
    "serve":    (cmd_serve, "Start the network server"),
}

BENCH_HELP = """\
Usage: run.py bench <bench_name> [mode]

  secret_recovery server|client   Secret-recovery benchmark
  sa_nivss        server|client   SA-NIVSS (saVSS) benchmark
  pv_nivss                        PV-NIVSS (cgVSS) benchmark

For backward compatibility, 'run.py bench server' and 'run.py bench client'
default to the secret_recovery benchmark.\
"""


def usage():
    print(__doc__)
    print("Commands:")
    for name, (_, desc) in COMMANDS.items():
        print(f"  {name:12s}  {desc}")
    print()
    print(BENCH_HELP)
    sys.exit(1)


def main():
    if len(sys.argv) < 2:
        usage()
    cmd = sys.argv[1]
    if cmd in ("-h", "--help"):
        usage()
    if cmd == "bench":
        if len(sys.argv) < 3:
            sys.exit(BENCH_HELP)
        arg2 = sys.argv[2]
        # Backward compatibility: 'bench server' / 'bench client'
        if arg2 in ("server", "client"):
            cmd_bench("secret_recovery", arg2)
        else:
            mode = sys.argv[3] if len(sys.argv) > 3 else None
            cmd_bench(arg2, mode)
    elif cmd in COMMANDS:
        COMMANDS[cmd][0]()
    else:
        sys.exit(f"Unknown command '{cmd}'.  Run with --help for usage.")


if __name__ == "__main__":
    main()
