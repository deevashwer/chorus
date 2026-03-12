#!/usr/bin/env python3
"""Generate Table 6: Secret-recovery client costs.

Usage:
    python3 experiments/generate_table6.py \
        --results-dir results/secret_recovery/<timestamp>
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    GROTH_PK_SIZE,
    latex_bytes,
    latex_time,
    process_client_log,
    process_server_log,
)


def generate(client_stats, server_stats, output_dir):
    key = next(iter(client_stats))
    cs = client_stats[key]
    ss = server_stats[key]

    backup_time = cs["Backup Time"]["avg_time"] if cs["Backup Time"] else 0
    backup_comm = ss["Backup Ciphertext"]
    recovery_time = cs["Recover Time"]["avg_time"] + (
        cs["Recovery Request Time"]["avg_time"] if cs["Recovery Request Time"] else 0
    )
    recovery_comm = ss["Recovery Request"] + ss["S->C Recovery Response"] + GROTH_PK_SIZE

    lines = [
        r"\begin{table}[t]",
        r"\centering",
        r"\caption{Secret-recovery client costs. Battery measurement skipped (not running on a mobile phone).} \label{tab:sr-client}",
        r"\begin{tabular}{lrr}",
        r"\toprule",
        r"\textbf{Operation} & \textbf{Time} & \textbf{Comm.} \\",
        r"\midrule",
        f"Backup    & {latex_time(backup_time)} & {latex_bytes(backup_comm)} \\\\",
        f"Recovery  & {latex_time(recovery_time)} & {latex_bytes(recovery_comm)} \\\\",
        r"\bottomrule",
        r"\end{tabular}",
        r"\end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "table6.tex"
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
