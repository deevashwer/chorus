#!/usr/bin/env python3
"""Generate Appendix A.4.1: One-time DKG setup costs.

Usage:
    python3 experiments/generate_appendixA41.py \
        --results-dir results/secret_recovery/<timestamp>
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    add_dicts,
    latex_bytes,
    latex_time,
    process_client_log,
    process_server_log,
)


def generate(client_stats, server_stats, output_dir):
    key = next(iter(client_stats))
    cs = client_stats[key]
    ss = server_stats[key]
    n = cs["Committee Size"]
    zero = {"avg_time": 0.0, "unit": "s"}

    # Epoch 0: 2 sortitions + DKG contribute
    client_time_0 = add_dicts(
        [cs["Sortition Time"] or zero] * 2 + [cs["DKG Contribute Time"] or zero]
    )
    client_comm_0 = (
        2 * ss["C->S Committee Selection"]
        + ss["C->S DKG-phase-1"]
        + 2 * ss["Committee"]
    )
    server_time_0 = add_dicts(
        [ss["Process Committee Time"]] * 2 + [ss["DKG-step-1 Time"]]
    )
    server_comm_0 = n * client_comm_0

    # Epoch 1: 1 sortition + DKG handover
    client_time_1 = add_dicts(
        [cs["Sortition Time"] or zero] + [cs["DKG Handover Time"] or zero]
    )
    client_comm_1 = (
        ss["Max Commstate_1"]
        + ss["C->S Committee Selection"]
        + ss["C->S DKG-phase-2"]
        + 3 * ss["Committee"]
    )
    server_time_1 = add_dicts(
        [ss["Process Committee Time"], ss["DKG-step-2 Time"]]
    )
    server_comm_1 = n * client_comm_1

    lines = [
        r"\begin{table}[h!]",
        r"\centering",
        r"\begin{tabular}{lrrrr}",
        r"\toprule",
        r"& \multicolumn{2}{c}{\textbf{--- Time ---}} & \multicolumn{2}{c}{\textbf{\quad\quad--- Comm ---}}   \\",
        r"& Client & Server & Client & Server \\ \midrule",
        f"\\textbf{{Epoch 0}} & {latex_time(client_time_0['avg_time'])} "
        f"& {latex_time(server_time_0['avg_time'])} "
        f"& {latex_bytes(client_comm_0)} "
        f"& {latex_bytes(server_comm_0)} \\\\",
        f"\\textbf{{Epoch 1}} & {latex_time(client_time_1['avg_time'])} "
        f"& {latex_time(server_time_1['avg_time'])} "
        f"& {latex_bytes(client_comm_1)} "
        f"& {latex_bytes(server_comm_1)} \\\\",
        r"\bottomrule",
        r"\end{tabular}",
        r"\end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "appendixA41.tex"
    out.write_text(tex)
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
