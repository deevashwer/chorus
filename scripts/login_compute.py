#!/usr/bin/env python3
"""SSH into the compute VM from the control VM.

Usage:
    python3 ~/chorus/scripts/login_compute.py
"""

import json
import os
import urllib.request
from pathlib import Path

CONFIG_PATH = Path(__file__).resolve().parent.parent / "config.json"

with open(CONFIG_PATH) as _f:
    CONFIG = json.load(_f)

COMPUTE_VM_NAME = CONFIG["compute_vm"]["name"]


def metadata(path: str) -> str:
    url = f"http://metadata.google.internal/computeMetadata/v1/{path}"
    req = urllib.request.Request(url, headers={"Metadata-Flavor": "Google"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        return resp.read().decode().strip()


def main():
    project = metadata("project/project-id")
    zone = metadata("instance/zone").rsplit("/", 1)[-1]

    print()
    print(f"  Connecting to compute VM '{COMPUTE_VM_NAME}'...")
    print(f"  Project: {project}  Zone: {zone}")
    print()

    os.execvp("gcloud", [
        "gcloud", "compute", "ssh", COMPUTE_VM_NAME,
        "--project", project,
        "--zone", zone,
    ])


if __name__ == "__main__":
    main()
