# Chorus: Secret Recovery with Ephemeral Client Committees

**IEEE S&P 2026**

Rust implementation and reproducibility package for the Chorus paper.

Reproducing the experiments requires two Ubuntu 22.04 VMs.  The
paper's client experiments ran on an Android phone (Snapdragon 8
Gen 3); the VM setup emulates network conditions (75 Mbps, 280 ms RTT)
with Linux `tc` instead, so battery measurements are skipped.
Instructions for running on Android are provided at the bottom of
this README.

---

## Getting Started

Provision two Ubuntu 22.04 machines that can reach each other over the
network.

| VM | Role | vCPUs | RAM | Disk |
|----|------|-------|-----|------|
| **Compute** | runs server benchmarks | 112 | 224 GB | 200 GB |
| **Control** | runs client benchmarks | 8 | 8 GB | 50 GB |

Requirements:
- Both VMs run **Ubuntu 22.04** (other Debian-based distros may work but are untested).
- SSH access from your local machine to the control VM.
- SSH access from your local machine to the compute VM.
- Network connectivity between the two VMs.

### Workflow

Your local machine needs only Python 3 and an SSH client.
Everything else is set up automatically by the scripts.

**Step 1 — Log in** (on your local machine):

```bash
python3 scripts/login.py
```

This asks for the control VM's IP, SSH user, and key, as well as the
compute VM's details, and copies the SSH key to the control VM so it
can reach the compute VM.  Then:
- On first run: syncs the repo to the control VM and installs `screen`
- Opens a persistent screen session (detach with Ctrl-A D, reconnect
  by re-running `login.py`)

**Step 2 — Set up both VMs** (on the control VM):

```bash
python3 ~/chorus/scripts/setup_eval.py
```

This does everything automatically:
1. Configures the compute VM connection if not already set by
   `login.py`.
2. Installs system packages, Rust, and builds the artifact on **both
   VMs in parallel**.
3. Generates benchmark state on the compute VM.

Estimated wall time: **~3.5 hours** (dominated by state generation on
the compute VM; deps & build run in parallel on both VMs).

**Step 3 — Run experiments** (on the control VM):

```bash
python3 ~/chorus/scripts/run_experiment.py all
```

Runs all experiments (~7 h total).  See the [Experiments](#experiments)
table below.

> Note:
> If SSH works but TCP connections to the compute VM's external IP on
> port `32000` are blocked, `run_experiment.py` may ask for an internal
> IP during the client benchmark portion of this step and cache it for
> future runs.

### Tear Down

Shut down or delete your VMs through your cloud provider's console
when you are done.

### Reconnecting

Experiments run inside a screen session and survive disconnections.
Two convenience scripts let you reconnect at any time:

| Script | Where to run | What it does |
|--------|-------------|--------------|
| `python3 scripts/login.py` | local machine | Reconnects to the control VM screen session |
| `python3 ~/chorus/scripts/login_compute.py` | control VM | SSHes into the compute VM (for debugging) |

---

## What the Setup Does (Manual Reference)

If you prefer to run the steps manually, clone the repo on both VMs
into `~/chorus` and run:

**On both VMs:**

```bash
bash scripts/setup_deps.sh        # system packages + Rust
python3 scripts/run.py build      # cargo build --release
```

**On the compute VM only:**

```bash
python3 scripts/run.py generate   # pre-generate benchmark state (~3 h)
```

**Networking:** The control VM must be able to SSH into the compute VM.
Create `vm_config.json` in the repo root with the compute VM's
connection details:

```json
{
  "mode": "manual",
  "compute": { "host": "<IP>", "user": "<USER>", "key": "<PATH_TO_KEY>" }
}
```

`run_experiment.py` reads this file to SSH into the compute VM, start
server binaries, set up network emulation (`tc`), and pass the
compute VM's IP to the client via the `SERVER_IP` environment
variable.

If client benchmarks cannot reach the compute VM on the configured
external IP and port `32000`, `run_experiment.py` will prompt for an
internal IP during the client benchmark step.

---

## Experiments

You can run experiments interactively or by ID:

```bash
python3 ~/chorus/scripts/run_experiment.py            # interactive menu
python3 ~/chorus/scripts/run_experiment.py table6      # run one experiment
python3 ~/chorus/scripts/run_experiment.py all         # run everything
python3 ~/chorus/scripts/run_experiment.py all --force # re-run from scratch
```

Benchmark logs are cached in `results/` and reused across runs.

| # | ID | Paper Reference | Est. time |
|---|-----|-----------------|-----------|
| 1 | `figure5` | Figure 5: saVSS vs cgVSS | ~2 h 28 min |
| 2 | `table9` | Table 9: Server per-epoch costs | ~3 h 30 min |
| 3 | `table6` | Table 6: Client secret-recovery costs | ~55 min* |
| 4 | `table7` | Table 7: Committee-member costs | < 1 min* |
| 5 | `figure8` | Figure 8: Client cost breakdown | < 1 min* |
| 6 | `appendixA41` | Appendix A.4.1: DKG setup costs | < 1 min* |
| 7 | `server_cost` | Server dollar-cost estimation | < 1 min* |
| 8 | `table10` | Table 10: Parameter selection | < 1 min |

\* Experiments 2–7 share benchmark logs.  Once `table9` and `table6`
have run, experiments 4–7 reuse those logs and finish instantly.

### Downloading Results

Results are saved on the control VM under `~/chorus/results/`.
To download them to your local machine:

```bash
python3 scripts/fetch_results.py                 # fetch all
python3 scripts/fetch_results.py table6 figure8  # fetch specific ones
```

Downloaded `.tex` tables and `.png` figures can be opened directly.

---

## Android Benchmarking

The paper's client-side experiments were run on an Android phone
(Qualcomm Snapdragon 8 Gen 3 processor, 8 cores, 8 GB RAM, 3000 mAh
battery). The following instructions describe how to cross-compile and
run the benchmarks on an Android device.

### Prerequisites

You need:
- A host machine (Linux or macOS) with the Chorus repo built
- An Android device connected via USB with **USB debugging** enabled
  in Developer Options
- The benchmark state directories generated on the server (see the
  main setup instructions above)

### Android NDK Setup

[Download](https://developer.android.com/ndk/downloads) the suitable
NDK package for your host machine into the `chorus` directory and
unpack it there.

The following assumes the target Android device runs Android 14 with a
64-bit ARM CPU (instruction set `aarch64`), requiring `api_level=34`
and the `arm64-v8a` ABI.  Adjust if your device differs.

> If you change the Android API level, update these files:
> `chorus/.cargo/config.toml`, `chorus/class_group/.cargo/config.toml`,
> `chorus/gmp-mpfr-sys/build.rs`, and `chorus/class_group/build.rs`.

Set up the following environment variables:

```bash
# See https://developer.android.com/ndk/guides/abis for more options
export ANDROID_ABI=arm64-v8a

# Linux
export ANDROID_NDK_HOME={chorus_dir}/android-ndk-*/
export ANDROID_TOOLCHAIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin

# macOS
export ANDROID_NDK_HOME={chorus_dir}/AndroidNDK*.app/Contents/NDK/
export ANDROID_TOOLCHAIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin

# Add the Android toolchain to the PATH
export PATH="$ANDROID_TOOLCHAIN:$PATH"
```

### Cross-Compilation

Install the Android target via rustup and build:

```bash
rustup target add aarch64-linux-android

# First build class_group crate
cd class_group; cargo build --target aarch64-linux-android --release

# Then build the chorus crate
cd chorus; cargo build --target aarch64-linux-android --release
```

Build the `secret_recovery` client benchmark binary:

```bash
cargo bench --target aarch64-linux-android --bench secret_recovery --no-run
```

This generates a binary in `target/aarch64-linux-android/release/deps/`,
e.g. `secret_recovery-abcd`.

### Deploying and Running on Android via adb

Install `adb` (Android Debug Bridge):

```bash
# Linux
sudo apt install adb

# macOS
brew install android-platform-tools
```

Transfer the benchmark binary and state directories to the device:

```bash
# Connect your Android device via USB and enable USB debugging
adb attach
adb push target/aarch64-linux-android/release/deps/secret_recovery-abcd /data/local/tmp
adb push case_*_clients_* /data/local/tmp
adb shell
```

Inside the adb shell on the Android device:

```bash
cd /data/local/tmp
BENCHMARK_TYPE=CLIENT ./secret_recovery-abcd --bench 2>&1 | tee secret_recovery_client.log
```

