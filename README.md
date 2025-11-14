# Chorus: Secret Recovery with Ephemeral Client Committees

## 📄 Artifact Material

For the full constructions, formal definitions, and security proofs deferred from the main paper, please refer to the artifact material:

**[chorus-artifact-material.pdf](./chorus-artifact-material.pdf)**

---

## 💻 Implementation & Benchmarking

The sections below provide instructions for building, running, and benchmarking the Chorus implementation.

## 📦 Installation

```bash
  # Install cargo
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # Install dependencies
  sudo apt-get install libgmp-dev libmpfr-dev libssl-dev m4
  # First build class_group crate
  cd class_group; cargo build --release
  # Then build the chorus crate
  cd chorus; cargo build --release
```

## 🚀 Benchmarking
### ️☁️ Server
To benchmark the server, run in the `chorus` directory:
```bash
# Run on server and log the output
BENCHMARK_TYPE=SERVER cargo bench --bench secret_recovery 2>&1 | tee secret_recovery_server.log
```

### 💻‍ Client

First run the following command in `chorus` directory on a server to store the server state to independently benchmark the end-to-end latency of a client:
```bash
# Run on server
BENCHMARK_TYPE=SAVE_STATE cargo bench --bench secret_recovery
```
This will create directories `./case_{case_number}_clients_{num_clients}` where `case_number = {1, 2}` and `num_clients = {1M, 10M, 100M}`. The case descriptions are as follows:
- `case_1`: committee size: 1090, threshold: 300, corruption fraction: 0.1, fail fraction: 0.5
- `case_2`: committee size: 1214, threshold: 121, corruption fraction: 0.01, fail fraction: 0.75

Then, run the the following on the server in the `chorus` directory to start a server listening on port 32000  after making sure that connections are accepted on this port and the `src/network.rs` file only includes the `CASES` and `NUM_CLIENTS` for which the state was saved:
```bash
# Run on server
./target/release/server
```
Transfer the `case_{case_number}_clients_{num_clients}` directories to the client and update `NETWORK_IP_FOR_CLIENTS` in `benches/secret_recovery.rs` on the client to `{SERVER_IP_ADDRESS}`.

Then run the following in the `chorus` directory:
```bash
# Run on client and log the output
BENCHMARK_TYPE=CLIENT cargo bench --bench secret_recovery 2>&1 | tee secret_recovery_client.log
```

> Note: the client benchmark can also run without network. For that, replace client benchmark that end in `with_network` with their local counterparts.

### 📱 Client (Android)
[Download](https://developer.android.com/ndk/downloads) the suitable NDK package for your host machine to the `chorus` directory, and unpack the downloaded archive into the same directory.

The following will assume that the target Android device runs Android 14 and has a 64-bit ARM CPU (instruction set=`aarch64`). As such, it requires `api_level=34` and `arm64-v8a` ABI. If you want to build for a different target, please adjust the following instructions accordingly.

> In case you update android API level, make sure to update the following files: `chorus/.cargo/config.toml`, `chorus/class_group/.cargo/config.toml`, `chorus/gmp-mpfr-sys/build.rs`, and `chorus/class_group/build.rs`.

Setup the following environment variables:
```bash
# See https://developer.android.com/ndk/guides/abis for more options
export ANDROID_ABI=arm64-v8a

# Linux
export ANDROID_NDK_HOME={chorus_dir}/android-ndk-*/
export ANDROID_TOOLCHAIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin

# MacOS
export ANDROID_NDK_HOME={chorus_dir}/AndroidNDK*.app/Contents/NDK/
export ANDROID_TOOLCHAIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin

# Add the Android toolchain to the PATH
export PATH="$ANDROID_TOOLCHAIN:$PATH"
```

Install Android toolchain using rustup: `rustup target add aarch64-linux-android`, and then build using this toolchain:
```bash
# First build class_group crate
cd class_group; cargo build --target aarch64-linux-android --release
# Then build the chorus crate
cd chorus; cargo build --target aarch64-linux-android --release
```
Build `secret_recovery` client benchmark:
```bash
cargo bench --target aarch64-linux-android --bench secret_recovery --no-run
```
The above command will generate a binary in `target/aarch64-linux-android/release/deps/`, say `secret_recovery-abcd`. Now, you can transfer this binary to your Android device using `adb` (Android Debug Bridge). To install `adb`, do the following:
```bash
# Linux
sudo apt install adb

# MacOS
brew install android-platform-tools
```
With `adb` installed, you can transfer the binary to your Android device as follows:
```bash
# Connect your Android device to your computer via USB and enable USB debugging in the developer options.
adb attach
adb push target/aarch64-linux-android/release/deps/secret-recovery-abcd /data/local/tmp
adb push case_*_clients_* /data/local/tmp
adb shell
```
This will give you a shell on the Android device. You can now run the binary:
```bash
# Run on the Android phone and log the output
cd /data/local/tmp
BENCHMARK_TYPE=CLIENT ./secret_recovery-abcd --bench 2>&1 | tee secret_recovery_client.log
```

## 🔍 Log Parsing
To parse the secret_recovery benchmark, copy `secret_recovery_server.log` and `secret_recovery_client.log` from the server and client, respectively, and run:
```bash
python parse_secret_recovery_bench.py
```