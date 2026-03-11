"""Shared parsing and formatting utilities for secret-recovery benchmark logs.

Used by generate_table6.py, generate_table7.py, generate_figure8.py,
generate_table9.py, and generate_appendixA41.py.
"""

import math
import re
from collections import defaultdict

GROTH_PK_SIZE = 205090704  # constant proving-key size (bytes)


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------

def convert_bytes(size):
    if size is None or size == 0:
        return "0 B"
    size = float(size)
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if abs(size) < 1000:
            if abs(size) >= 100:
                return f"{size:.0f} {unit}"
            elif abs(size) >= 10:
                return f"{size:.1f} {unit}"
            else:
                return f"{size:.2f} {unit}"
        size /= 1000
    return f"{size:.2f} PB"


def latex_time(seconds):
    if seconds < 1:
        ms = seconds * 1000
        if ms < 0.1:
            return f"${ms:.3f}$ ms"
        elif ms < 10:
            return f"${ms:.1f}$ ms"
        else:
            return f"${ms:.0f}$ ms"
    if seconds >= 100:
        return f"${seconds:.0f}$ s"
    return f"${seconds:.1f}$ s"


def latex_bytes(size_bytes):
    if size_bytes is None or size_bytes == 0:
        return "$0$ B"
    size = float(size_bytes)
    for unit in ["B", "KB", "MB", "GB", "TB"]:
        if abs(size) < 1000:
            if abs(size) >= 100:
                return f"${size:.0f}$ {unit}"
            elif abs(size) >= 10:
                return f"${size:.1f}$ {unit}"
            else:
                return f"${size:.2f}$ {unit}"
        size /= 1000
    return f"${size:.2f}$ PB"


def readable_time(minutes):
    months = minutes // (30 * 24 * 60)
    days = (minutes % (30 * 24 * 60)) // (24 * 60)
    hours = (minutes % (24 * 60)) // 60
    remaining = minutes % 60

    parts = []
    if months > 0:
        parts.append(f"${months}$ mo")
    if days > 0:
        parts.append(f"${days}$ d")
    if hours > 0:
        parts.append(f"${hours}$ h")
    if remaining > 0 and not parts:
        parts.append(f"${remaining}$ min")
    return " ".join(parts) if parts else "$0$ min"


def n_label(n):
    if n <= 0:
        return "$N=0$"
    exp = round(math.log10(n))
    return f"$N=10^{{{exp}}}$"


def add_dicts(dicts):
    result = {"avg_time": 0.0, "unit": dicts[0]["unit"]}
    for d in dicts:
        result["avg_time"] += d["avg_time"]
    return result


# ---------------------------------------------------------------------------
# Log-parsing helpers
# ---------------------------------------------------------------------------

def get_time(bench, log):
    time_re = re.compile(
        re.escape(bench)
        + r"(?:[\s\S]*?)time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]"
    )
    matches = time_re.findall(log)
    extracted = []
    for unit, val in matches:
        avg_time = float(val)
        if unit == "ms":
            unit = "s"
            avg_time /= 1000.0
        extracted.append({"avg_time": avg_time, "unit": unit})
    return extracted if extracted else None


def get_time_and_battery(bench, log):
    pat = re.compile(
        re.escape(bench)
        + r"(?:[\s\S]*?)time:\s+\[\d+\.\d+\s([a-z]+)\s(\d+\.\d+)\s[a-z]+\s\d+\.\d+\s[a-z]+\]",
        re.MULTILINE | re.DOTALL,
    )
    m = pat.search(log)
    if m:
        unit = m.group(1)
        avg_time = float(m.group(2))
        if unit == "ms":
            unit = "s"
            avg_time /= 1000.0
        return {"avg_time": avg_time, "unit": unit}
    return None


def extract_bytesize(log, metric_name):
    pat = re.compile(re.escape(metric_name) + r" bytesize:\s+(\d+)")
    m = pat.search(log)
    return int(m.group(1)) if m else 0


# ---------------------------------------------------------------------------
# Client log parsing
# ---------------------------------------------------------------------------

def process_client_log(log_path):
    with open(log_path) as f:
        log = f.read()

    runs = log.split("case: ")[1:]
    info_re = re.compile(
        r"\s*(\d+),\s*corrupt fraction:\s*(\d+),\s*fail fraction:\s*(\d+),"
        r"\s*threshold:\s*(\d+),\s*committee_size:\s*(\d+),\s*num_clients:\s*(\d+)"
    )

    stats = {}
    for run_text in runs:
        info = info_re.search(run_text)
        if not info:
            continue
        case, corrupt, fail, threshold, committee_size, num_clients = info.groups()
        case = case.strip()
        num_clients = int(num_clients)
        committee_size = int(committee_size)
        threshold = int(threshold)

        sortition = get_time_and_battery("sortition", run_text)
        dkg_contribute = get_time_and_battery("dkg-contribute", run_text)
        dkg_handover = get_time_and_battery("handover-dkg", run_text)
        typical_handover = get_time_and_battery("handover-typical", run_text)
        backup = get_time_and_battery("backup", run_text)
        recovery_request = get_time_and_battery("recovery-request", run_text)
        recover = {"avg_time": 0.0, "unit": "s"}

        expected_req_re = re.compile(r"expected_requests_per_epoch:\s+(\d+)")
        m = expected_req_re.search(run_text)
        expected_requests = int(m.group(1)) if m else 0

        trace_pattern = r"End:\s+(.+?)\s+\.+([0-9.]+)(s|ms|µs|ns)"
        time_data = defaultdict(lambda: {"total": 0.0, "count": 0})
        conversion = {"s": 1, "ms": 1e-3, "µs": 1e-6, "ns": 1e-9}
        for task, value, unit in re.findall(trace_pattern, run_text):
            time_data[task]["total"] += float(value) * conversion[unit]
            time_data[task]["count"] += 1

        for key in time_data:
            if time_data[key]["count"] > 0:
                time_data[key]["avg"] = time_data[key]["total"] / time_data[key]["count"]
            else:
                time_data[key]["avg"] = 0.0

        def td_avg(k):
            return time_data[k]["avg"] if k in time_data else 0.0

        stat = {
            "Corrupt Fraction": int(corrupt),
            "Fail Fraction": int(fail),
            "Threshold": threshold,
            "Committee Size": committee_size,
            "Num Clients": num_clients,
            "Sortition Time": sortition,
            "DKG Contribute Time": dkg_contribute,
            "DKG Handover Time": dkg_handover,
            "Typical Handover Time": typical_handover,
            "Backup Time": backup,
            "Recovery Request Time": recovery_request,
            "Recover Time": recover,
            "Expected Requests": expected_requests,
            "Committee_Selection Time": td_avg("verify next committee") + td_avg("verify prev committee"),
            "Share Time": td_avg("reshare"),
            "Reconstruct Time": td_avg("verify consistency between states") + td_avg("receive shares"),
            "Process Recovery Request Time": (
                td_avg("verify recovery request nizks")
                + td_avg("verify merkle proof")
                + td_avg("compute recovery response")
            ),
            "DKG Contribute Comm Time": td_avg("download for dkg-contribute") + td_avg("upload for dkg-contribute"),
            "DKG Handover Comm Time": td_avg("download for handover-dkg") + td_avg("upload for handover-dkg"),
            "Typical Handover Comm Time": td_avg("download for handover-typical") + td_avg("upload for handover-typical"),
        }
        stats[(case, num_clients)] = stat
    return stats


# ---------------------------------------------------------------------------
# Server log parsing
# ---------------------------------------------------------------------------

def process_server_log(log_path):
    with open(log_path) as f:
        log = f.read()

    runs = log.split("case: ")[1:]
    info_re = re.compile(
        r"\s*(\d+),\s*corrupt fraction:\s*(\d+),\s*fail fraction:\s*(\d+),"
        r"\s*threshold:\s*(\d+),\s*committee_size:\s*(\d+),\s*num_clients:\s*(\d+)"
    )

    stats = {}
    for run_text in runs:
        info = info_re.search(run_text)
        if not info:
            continue
        case, corrupt, fail, threshold, committee_size, num_clients = info.groups()
        case = case.strip()
        num_clients = int(num_clients)
        committee_size = int(committee_size)
        threshold = int(threshold)

        pct = get_time("process-committee", run_text)
        pst = get_time("process-state", run_text)
        prq = get_time("server-process-recovery-requests", run_text)
        prs = get_time("server-process-recovery-responses", run_text)

        zero = {"avg_time": 0.0, "unit": "s"}

        stat = {
            "Process Committee Time": pct[0] if pct else zero,
            "DKG-step-1 Time": pst[0] if pst and len(pst) > 0 else zero,
            "DKG-step-2 Time": pst[1] if pst and len(pst) > 1 else zero,
            "Handover Time": pst[2] if pst and len(pst) > 2 else zero,
            "Process Recovery Request Time": prq[0] if prq else zero,
            "Process Recovery Response Time": prs[0] if prs else zero,
            "C->S Committee Selection": extract_bytesize(run_text, "Max Nomination"),
            "C->S DKG-phase-1": extract_bytesize(run_text, "Max Contribution"),
            "C->S DKG-phase-2": extract_bytesize(run_text, "Max Handover (DKG)"),
            "C->S Handover": extract_bytesize(run_text, "Max Handover (Typical)"),
            "C->S Recovery Response Batch": extract_bytesize(run_text, "Max Recovery Response Batch"),
            "S->C Recovery Response": extract_bytesize(run_text, "Max Recovery Responses"),
            "Backup Ciphertext": extract_bytesize(run_text, "Backup Ciphertext"),
            "Recovery Request": extract_bytesize(run_text, "Recovery Request"),
            "Recovery Request Batch": extract_bytesize(run_text, "Recovery Request Batch"),
            "Committee": extract_bytesize(run_text, "committee_0"),
            "Public State": extract_bytesize(run_text, "public_state_epoch_2"),
            "Max Commstate_1": extract_bytesize(run_text, "Max commstate_1"),
            "Max Commstate_2": extract_bytesize(run_text, "Max commstate_2"),
            "Max Commstate_3": extract_bytesize(run_text, "Max commstate_3"),
            "Prev State Coeffs": extract_bytesize(run_text, "prev_state"),
            "Corrupt Fraction": int(corrupt),
            "Fail Fraction": int(fail),
            "Threshold": threshold,
            "Committee Size": committee_size,
            "Num Clients": num_clients,
        }

        req_re = re.compile(r"expected_requests_per_epoch:\s+(\d+)")
        m = req_re.search(run_text)
        stat["Expected Requests"] = int(m.group(1)) if m else 0

        stats[(case, num_clients)] = stat
    return stats
