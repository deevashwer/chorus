#!/usr/bin/env python3
"""Plot Figure 5: saVSS vs cgVSS (NIVSS) benchmark results.

Usage:
    python3 experiments/plot_nivss.py --results-dir experiments/results/figure5/<timestamp>

Expects three log files in the results directory:
    sa_nivss_server.log
    sa_nivss_client_parallel.log
    pv_nivss_client_parallel.log

Outputs (in the same directory):
    nivss-time.png
    nivss-comm.png
    nivss-legend.png
"""

import argparse
import json
import re
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")  # non-interactive backend (no display needed)
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

# Load config for receive_parallel_count
_CONFIG_PATH = Path(__file__).resolve().parent.parent / "config.json"
with open(_CONFIG_PATH) as _f:
    _CONFIG = json.load(_f)
RECEIVE_PARALLEL = _CONFIG.get("nivss_receive_parallel", 8)


# ---------------------------------------------------------------------------
# Log parsing helpers
# ---------------------------------------------------------------------------

def get_time(bench_name, log_text):
    """Extract the median time from a criterion benchmark block."""
    pattern = re.compile(
        bench_name + r".*time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]"
    )
    match = pattern.search(log_text)
    if match:
        unit = match.group(1)
        avg_time = float(match.group(2))
        if unit == "ms":
            avg_time /= 1000
            unit = "s"
        return {"avg_time": avg_time, "unit": unit}
    return None


def convert_bytes(size):
    """Convert bytes to a human-readable string."""
    if size is None:
        return None
    size = float(size)
    for unit in ["bytes", "KB", "MB", "GB", "TB"]:
        if size < 1000:
            return f"{size:.2f} {unit}"
        size /= 1000
    return f"{size:.2f} PB"


# ---------------------------------------------------------------------------
# Client log parsing (pv_nivss & sa_nivss CLIENT)
# ---------------------------------------------------------------------------

def process_client_log(log_path):
    """Parse a client benchmark log and return per-case stats."""
    with open(log_path) as f:
        log = f.read()

    cases = log.split("case: ")[1:]  # skip preamble

    info_re = re.compile(
        r"fraction:\s*(\d+),\s*committee_size:\s*(\d+),\s*threshold:\s*(\d+)"
    )
    dealing_size_re = re.compile(r"Dealing bytesize:\s+(\d+)")
    lite_dealing_size_re = re.compile(r"Max Lite Dealing bytesize:\s+(\d+)")

    stats = {}
    for idx, case_text in enumerate(cases):
        info = info_re.search(case_text)
        if info is None:
            continue
        fraction, committee_size, threshold = (
            int(info.group(1)),
            int(info.group(2)),
            int(info.group(3)),
        )

        deal_time = get_time("deal", case_text)
        receive_time = get_time("receive", case_text)

        dealing_size = dealing_size_re.search(case_text)
        dealing_size = int(dealing_size.group(1)) if dealing_size else None

        lite_dealing_size = lite_dealing_size_re.search(case_text)
        lite_dealing_size = int(lite_dealing_size.group(1)) if lite_dealing_size else None

        stats[idx] = {
            "Fraction": fraction,
            "Threshold": threshold,
            "Committee Size": committee_size,
            "Deal Time": deal_time["avg_time"] if deal_time else None,
            # Divide by parallel receive count from config
            "Receive Time": (receive_time["avg_time"] / RECEIVE_PARALLEL) if receive_time else None,
            "Dealing Size": (dealing_size / 1e9) if dealing_size else None,
            "Lite Dealing Size": (lite_dealing_size / 1e9) if lite_dealing_size else None,
        }
    return stats


# ---------------------------------------------------------------------------
# Server log parsing (sa_nivss SERVER)
# ---------------------------------------------------------------------------

def process_server_log(log_path):
    """Parse a server benchmark log and return per-case stats."""
    with open(log_path) as f:
        log = f.read()

    cases = log.split("case: ")[1:]

    info_re = re.compile(
        r"fraction:\s*(\d+),\s*committee_size:\s*(\d+),\s*threshold:\s*(\d+)"
    )

    stats = {}
    for idx, case_text in enumerate(cases):
        info = info_re.search(case_text)
        if info is None:
            continue
        fraction, committee_size, threshold = (
            int(info.group(1)),
            int(info.group(2)),
            int(info.group(3)),
        )

        verify_time = get_time("verify-dealing", case_text)

        stats[idx] = {
            "Fraction": fraction,
            "Threshold": threshold,
            "Committee Size": committee_size,
            "Verify Dealing Time": verify_time["avg_time"] if verify_time else None,
        }
    return stats


# ---------------------------------------------------------------------------
# Plotting
# ---------------------------------------------------------------------------

def improvement(orig, imp):
    return [o / i if i != 0 else float("inf") for o, i in zip(orig, imp)]


def plot_times(pv_nivss, sa_nivss, sa_nivss_server, output_dir):
    cases = list(range(len(pv_nivss)))

    xticks = [pv_nivss[c]["Committee Size"] for c in cases]
    xlabels = [
        f"n={pv_nivss[c]['Committee Size']}\n"
        f"t={sa_nivss[c]['Threshold']}\n"
        rf"$\kappa$={pv_nivss[c]['Fraction']}%"
        for c in cases
    ]

    max_dealings_processed = [2 * pv_nivss[c]["Threshold"] for c in cases]

    pv_deal = [pv_nivss[c]["Deal Time"] for c in cases]
    pv_recv = [pv_nivss[c]["Receive Time"] * max_dealings_processed[c] for c in cases]

    sa_deal = [sa_nivss[c]["Deal Time"] for c in cases]
    sa_recv = [sa_nivss[c]["Receive Time"] * sa_nivss[c]["Threshold"] for c in cases]
    sa_verify = [sa_nivss_server[c]["Verify Dealing Time"] for c in cases]

    print("deal_time_improvement", improvement(pv_deal, sa_deal))
    print("recv_time_improvement", improvement(pv_recv, sa_recv))

    fig, ax = plt.subplots(figsize=(3, 3))
    ax.plot(xticks, pv_deal, marker="o", linestyle="-", color="tab:red", label="cgVSS Dealer")
    ax.plot(xticks, pv_recv, marker="x", linestyle="-", color="tab:red", label="cgVSS Recipient")
    ax.plot(xticks, sa_deal, marker="o", linestyle="--", color="tab:blue", label="saNIVSS Dealer")
    ax.plot(xticks, sa_recv, marker="x", linestyle="--", color="tab:blue", label="saNIVSS Recipient")
    ax.plot(xticks, sa_verify, marker="^", linestyle="--", color="tab:blue", label="saNIVSS Server")

    ax.set_yscale("log")
    ax.set_xscale("log", base=2)
    ax.set_xticks(xticks)
    ax.set_xticklabels(xlabels)
    ax.grid(which="major", axis="y", color="gray", linestyle=":", linewidth=0.5)

    fig.tight_layout()
    out = output_dir / "nivss-time.png"
    plt.savefig(out, bbox_inches="tight", dpi=150)
    plt.close(fig)
    print(f"  Saved {out}")


def plot_comms(pv_nivss, sa_nivss, output_dir):
    cases = list(range(len(pv_nivss)))

    xticks = [pv_nivss[c]["Committee Size"] for c in cases]
    xlabels = [
        f"n={pv_nivss[c]['Committee Size']}\n"
        f"t={sa_nivss[c]['Threshold']}\n"
        rf"$\kappa$={pv_nivss[c]['Fraction']}%"
        for c in cases
    ]

    max_dealings_recvd = [pv_nivss[c]["Committee Size"] for c in cases]

    pv_deal_comm = [pv_nivss[c]["Dealing Size"] for c in cases]
    pv_recv_comm = [pv_nivss[c]["Dealing Size"] * max_dealings_recvd[c] for c in cases]
    pv_server_comm = [
        pv_nivss[c]["Dealing Size"] * max_dealings_recvd[c]
        + pv_nivss[c]["Dealing Size"] * (max_dealings_recvd[c] ** 2)
        for c in cases
    ]

    sa_deal_comm = [sa_nivss[c]["Dealing Size"] for c in cases]
    sa_recv_comm = [
        (sa_nivss[c]["Lite Dealing Size"] or 0) * sa_nivss[c]["Threshold"]
        for c in cases
    ]
    sa_server_comm = [
        sa_nivss[c]["Dealing Size"] * max_dealings_recvd[c]
        + (sa_nivss[c]["Lite Dealing Size"] or 0) * sa_nivss[c]["Threshold"] * max_dealings_recvd[c]
        for c in cases
    ]

    print("deal_comm_improvement", improvement(pv_deal_comm, sa_deal_comm))
    print("recv_comm_improvement", improvement(pv_recv_comm, sa_recv_comm))
    print("server_comm_improvement", improvement(pv_server_comm, sa_server_comm))

    fig, ax = plt.subplots(figsize=(3, 3))

    y_vals = [10 ** i for i in range(-3, 5)]
    for y in y_vals:
        ax.axhline(y=y, color="gray", linestyle=":", linewidth=0.5, alpha=1)

    ax.plot(xticks, pv_deal_comm, marker="o", linestyle="-", color="tab:red", label="cgVSS Dealer")
    ax.plot(xticks, pv_recv_comm, marker="x", linestyle="-", color="tab:red", label="cgVSS Recipient")
    ax.plot(xticks, pv_server_comm, marker="^", linestyle="-", color="tab:red", label="cgVSS Server")
    ax.plot(xticks, sa_deal_comm, marker="o", linestyle="--", color="tab:blue", label="saNIVSS Dealer")
    ax.plot(xticks, sa_recv_comm, marker="x", linestyle="--", color="tab:blue", label="saNIVSS Recipient")
    ax.plot(xticks, sa_server_comm, marker="^", linestyle="--", color="tab:blue", label="saNIVSS Server")

    ax.set_yscale("log")
    ax.set_xscale("log", base=2)
    ax.set_xticks(xticks)
    ax.set_xticklabels(xlabels)
    ax.minorticks_on()
    ax.grid(which="major", axis="y", color="gray", linestyle=":", linewidth=0.5)

    fig.tight_layout()
    out = output_dir / "nivss-comm.png"
    plt.savefig(out, bbox_inches="tight", dpi=150)
    plt.close(fig)
    print(f"  Saved {out}")


def plot_legend(output_dir):
    fig, ax = plt.subplots(figsize=(6, 0.5))
    handles = [
        plt.Line2D([0], [0], marker="o", linestyle="-", color="tab:red", label="cgVSS Share"),
        plt.Line2D([0], [0], marker="o", linestyle="--", color="tab:blue", label="saVSS Share"),
        plt.Line2D([0], [0], marker="x", linestyle="-", color="tab:red", label="cgVSS Reconst."),
        plt.Line2D([0], [0], marker="x", linestyle="--", color="tab:blue", label="saVSS Reconst."),
        plt.Line2D([0], [0], marker="^", linestyle="-", color="tab:red", label="cgVSS Server"),
        plt.Line2D([0], [0], marker="^", linestyle="--", color="tab:blue", label="saVSS Server"),
    ]
    ax.legend(handles=handles, loc="center", ncol=3, frameon=True)
    ax.axis("off")
    out = output_dir / "nivss-legend.png"
    plt.savefig(out, bbox_inches="tight", dpi=150)
    plt.close(fig)
    print(f"  Saved {out}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Plot Figure 5: saVSS vs cgVSS (NIVSS) benchmark results."
    )
    parser.add_argument(
        "--results-dir", required=True, type=Path,
        help="Directory containing the three log files.",
    )
    args = parser.parse_args()

    results_dir = args.results_dir
    sa_server_log = results_dir / "sa_nivss_server.log"
    sa_client_log = results_dir / "sa_nivss_client_parallel.log"
    pv_client_log = results_dir / "pv_nivss_client_parallel.log"

    for p in (sa_server_log, sa_client_log, pv_client_log):
        if not p.exists():
            sys.exit(f"Missing log file: {p}")

    print("Parsing logs...")
    pv_client = process_client_log(pv_client_log)
    sa_client = process_client_log(sa_client_log)
    sa_server = process_server_log(sa_server_log)

    print(f"Generating plots in {results_dir} ...")
    plot_times(pv_client, sa_client, sa_server, results_dir)
    plot_comms(pv_client, sa_client, results_dir)
    plot_legend(results_dir)
    print("Done.")


if __name__ == "__main__":
    main()
