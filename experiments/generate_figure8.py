#!/usr/bin/env python3
"""Generate Figure 8: Client cost breakdown (stacked bar charts).

Usage:
    python3 experiments/generate_figure8.py \
        --results-dir results/secret_recovery/<timestamp>
"""

import argparse
import math
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    process_client_log,
    process_server_log,
)


def generate(client_stats, server_stats, output_dir):
    cases = sorted(set(c for c, _ in client_stats.keys()))
    case = cases[0]
    n_values = sorted([n for c, n in client_stats.keys() if c == case])

    default_cs = client_stats[(case, n_values[0])]

    breakdown = {}
    for n in n_values:
        cs = client_stats[(case, n)]
        ss = server_stats.get((case, n))
        if ss is None:
            continue

        stat = {
            "Retrieve Committees": {
                "Time": default_cs["Committee_Selection Time"],
                "Comm": (3 * ss["Committee"]) / 1e6,
            },
            "Handover": {
                "Time": default_cs["Share Time"] + default_cs["Reconstruct Time"],
                "Comm": (ss["C->S Handover"] + ss["Max Commstate_2"] + ss["Prev State Coeffs"]) / 1e6,
            },
            "Network": {
                "Time": cs["Typical Handover Comm Time"],
                "Comm": 0.0,
            },
            "Process Requests": {
                "Time": cs["Process Recovery Request Time"],
                "Comm": (ss["Recovery Request Batch"] + ss["C->S Recovery Response Batch"]) / 1e6,
            },
        }
        breakdown[n] = stat

    if not breakdown:
        print("  WARNING: No data for Figure 8 -- skipping.")
        return

    categories = list(breakdown[n_values[0]].keys())
    bar_labels = [f"$N=10^{{{round(math.log10(n))}}}$" for n in n_values]
    colors = ["skyblue", "salmon", "lightgreen", "gold"]

    for metric in ["Time", "Comm"]:
        data = []
        for cat in categories:
            data.append([breakdown[n][cat][metric] for n in n_values])

        for n in n_values:
            total = sum(breakdown[n][cat][metric] for cat in categories)
            print(f"    Total {metric} for N={n}: {total:.4f}")

        fig, ax = plt.subplots(figsize=(2.5, 2.5))
        positions = np.arange(len(bar_labels)) * 0.2
        bottom = np.zeros(len(bar_labels))
        for i, (cat, color) in enumerate(zip(categories, colors)):
            ax.bar(
                positions, data[i], bottom=bottom, label=cat,
                color=color, edgecolor="black", linewidth=1.0, width=0.1,
            )
            bottom += np.array(data[i])

        ax.set_xticks(positions)
        ax.set_xticklabels(bar_labels)
        max_height = max(bottom)
        ax.set_ylim(0, max_height * 1.05 if max_height > 0 else 1)
        plt.tight_layout()
        out = output_dir / f"client_breakdown_{metric}.png"
        plt.savefig(out, bbox_inches="tight", dpi=150)
        plt.close(fig)
        print(f"  Saved {out}")

    fig, ax = plt.subplots(figsize=(6, 0.5))
    for cat, color in zip(categories, colors):
        ax.bar(0, 0, color=color, label=cat)
    ax.legend(loc="center", ncol=2, frameon=True)
    ax.axis("off")
    out = output_dir / "client-breakdown-legend.png"
    plt.savefig(out, bbox_inches="tight", dpi=150)
    plt.close(fig)
    print(f"  Saved {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results-dir", required=True, type=Path)
    parser.add_argument("--output-dir", type=Path, default=None)
    args = parser.parse_args()

    results_dir = args.results_dir
    output_dir = args.output_dir or results_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    client_stats = process_client_log(results_dir / "secret_recovery_client.log")
    server_stats = process_server_log(results_dir / "secret_recovery_server.log")

    if not client_stats or not server_stats:
        sys.exit("No benchmark data found in logs.")

    generate(client_stats, server_stats, output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
