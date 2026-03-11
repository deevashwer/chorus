#!/usr/bin/env python3
"""Generate Figure 5: saVSS vs cgVSS (NIVSS) runtime and communication.

Usage:
    python3 experiments/generate_figure5.py \
        --results-dir results/figure5/<timestamp>

Expects three log files in the results directory:
    sa_nivss_server.log
    sa_nivss_client_parallel.log
    pv_nivss_client_parallel.log

Outputs (in --output-dir, defaulting to --results-dir):
    nivss-time.png
    nivss-comm.png
    nivss-legend.png
"""

import argparse
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_nivss import process_client_log, process_server_log  # noqa: E402


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
        description="Generate Figure 5: saVSS vs cgVSS (NIVSS) benchmark results."
    )
    parser.add_argument(
        "--results-dir", required=True, type=Path,
        help="Directory containing the three log files.",
    )
    parser.add_argument(
        "--output-dir", type=Path, default=None,
        help="Directory to write outputs (defaults to --results-dir).",
    )
    args = parser.parse_args()

    results_dir = args.results_dir
    output_dir = args.output_dir or results_dir
    output_dir.mkdir(parents=True, exist_ok=True)

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

    print(f"Generating plots in {output_dir} ...")
    plot_times(pv_client, sa_client, sa_server, output_dir)
    plot_comms(pv_client, sa_client, output_dir)
    plot_legend(output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
