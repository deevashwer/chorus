"""Shared parsing utilities for NIVSS benchmark logs.

Used by generate_figure5.py.
"""

import json
import re
from pathlib import Path

_CONFIG_PATH = Path(__file__).resolve().parent.parent / "config.json"
with open(_CONFIG_PATH) as _f:
    _CONFIG = json.load(_f)
RECEIVE_PARALLEL = _CONFIG["nivss_receive_parallel"]


# ---------------------------------------------------------------------------
# Log-parsing helpers
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

    cases = log.split("case: ")[1:]

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
