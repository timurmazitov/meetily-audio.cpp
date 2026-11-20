# Building Samobranka from Source

This guide explains source builds on each supported platform. Start with the build notes below, then use the platform instructions that match your machine.

## Build Notes

Install the committed frontend dependencies before building:

```bash
npm install -g pnpm@9.15.9
cd frontend
pnpm install --frozen-lockfile
```

Frozen installation keeps the lockfile and installed dependency set aligned. When intentionally changing dependencies, update and commit `pnpm-lock.yaml`.

- **Linux:** Samobranka is built from source; choose acceleration for the environment where you build.
- **Windows packages:** Distribution builds use Vulkan-enabled Whisper and require an AVX2-capable x64 CPU. AVX-512 is not required.
- **CUDA:** NVIDIA CUDA support requires a compatible source build and CUDA toolchain; the standard Windows installer does not select it automatically.

## Release Artifacts and Updates

Releases are produced by `.github/workflows/release.yml`, which builds Linux (`deb`, `appimage`) and Windows packages into one draft release. macOS is intentionally excluded: CI has no Apple Developer certificate, so signed macOS builds cannot be produced (re-add the `macos-latest` matrix entry in `release.yml` to restore).

Linux packages are signed with minisign: `tauri signer sign` writes `<package>.deb.sig` or `<package>.AppImage.sig` next to each file. These double as the updater signatures for the AppImage. Windows installers are Authenticode-signed by `signCommand`.

**Automatic updates are enabled.** The in-app updater serves updates from this repository's GitHub Releases: `frontend/src-tauri/tauri.conf.json` has `bundle.createUpdaterArtifacts: true`, a `plugins.updater` section (endpoint `…/timurmazitov/samobranka/releases/latest/download/latest.json` plus the minisign public key) and the `updater:default` permission. The release pipeline signs updater artifacts and publishes a `latest.json` manifest (see `scripts/check-release-assets.py`); the app checks it automatically on startup and via **Check for Updates** in the About dialog or tray.

Prerequisites for a release to be updatable: the `TAURI_SIGNING_PRIVATE_KEY` repository secret must contain the minisign private key (the matching public key is committed in `tauri.conf.json`), and the release must be **published** — GitHub's `releases/latest` URL ignores drafts and prereleases. Platform caveats: only the Linux AppImage auto-updates (the `.deb` never will), and `latest.json` covers Windows and Linux only until macOS builds are restored.

## Development Commands and Debugging

### First-run sidecar setup

Installing frontend dependencies does not prepare `llama-helper`. The desktop dev/build commands expect this sidecar to be staged first. Repeat preparation if the staged binary is removed or the build target changes.

From the repository root, build the helper and inspect the native host target:

```bash
cargo build -p llama-helper
rustc -vV
```

Use the value printed after `host:` as `<host-triple>` below. If the helper build fails, stop: do not copy a binary or launch Tauri.

Create `frontend/src-tauri/binaries/`, then copy the built executable using the paths for your platform, relative to the repository root. Replace `<host-triple>` with the actual value printed by `rustc`, not the literal placeholder:

| Platform | Built executable | Staged destination |
| --- | --- | --- |
| macOS/Linux | `target/debug/llama-helper` | `frontend/src-tauri/binaries/llama-helper-<host-triple>` |
| Windows | `target/debug/llama-helper.exe` | `frontend/src-tauri/binaries/llama-helper-<host-triple>.exe` |

For a production build, run `cargo build -p llama-helper --release` and copy the executable from `target/release/` to the same target-suffixed destination, only after the build succeeds.

These commands build the helper without an explicit GPU backend. Its optional acceleration features are `metal`, `cuda`, and `vulkan`, each requiring a compatible platform and toolchain. Do not forward Whisper's `coreml`, `hipblas`, or `openblas` flags to the helper.

These paths assume an ordinary native-host workspace build. With a custom Cargo output directory or cross-compilation, copy from the actual configured output and use the actual build target as the suffix instead of the `rustc` host.

### First-run transcribe.cpp runtime (transcribeCpp provider)

The `transcribeCpp` provider needs the transcribe.cpp native runtime (`libtranscribe.*` plus its ggml backend modules). Like the helper, it is not created by installing frontend dependencies; stage it once with the cross-platform script from `frontend/`:

```bash
cd frontend
pnpm run stage:transcribe            # download + SHA-256 verify + extract
pnpm run stage:transcribe -- --verify  # assert the staged files are complete
```

(`--verify` runs the script's verification mode: `node scripts/stage-transcribe-native.mjs --verify`.)

The script downloads the pinned transcribe.cpp **v0.4.2** release archive for the current platform/arch lane from GitHub Releases, verifies the archive against a committed SHA-256 pin, and extracts `libtranscribe.*`, the ggml backend modules (per-ISA CPU + Vulkan, or Metal on macOS), `contract.json` and `licenses/` flat into `frontend/src-tauri/transcribecpp-runtime/` (gitignored). Re-runs are skipped via a `.staged` stamp file; a hash mismatch deletes the partial download and fails.

| Platform/arch | Lane downloaded |
| --- | --- |
| Linux x86_64 | `linux-x86_64-cpu-vulkan` |
| Linux aarch64 | `linux-aarch64-cpu-vulkan` |
| Windows x64 | `windows-x86_64-cpu-vulkan` |
| macOS arm64 | `macos-arm64-metal` |

CI parity: the same script runs as build steps in `build.yml` (Linux) and `build-windows.yml` (Windows) before bundling, so local and CI trees stage identically. On Windows the directory is additionally mapped into the install root next to the executable via `tauri.windows.conf.json` (see the Windows section below); on Linux debug builds the app probes the staged directory automatically.

The pinned `contract.json` ABI is checked both at staging (early sanity check in the script) and at library load time (the authoritative gate in `transcribe_cpp_engine/wrapper.rs`). Upgrading transcribe.cpp means changing the version pin, every lane SHA-256 and the contract/FFI subset in one coordinated edit — see [ADR 0006](adr/0006-transcribe-cpp-as-fourth-engine.md).

**Vulkan note:** staging itself needs no SDK — the release lanes ship prebuilt Vulkan backend modules. Unlike Whisper's Vulkan *build* (which requires the repository-local Vulkan SDK in `.tools/`, above), running transcribe.cpp on Vulkan only needs a working Vulkan driver/loader at runtime (`libvulkan.so.1` on Linux, `vulkan-1.dll` on Windows, installed with GPU drivers). Without a usable driver the engine falls back to CPU with a user-facing warning; see [RUNNING_LINUX.md](RUNNING_LINUX.md) / [RUNNING_WINDOWS.md](RUNNING_WINDOWS.md).

### Local LLVM toolchain (`.tools/`)

The build requires **libclang** at compile time: `bindgen` (pulled in by `llama-cpp-sys-2` for `llama-helper` and by `whisper-rs-sys`) generates FFI bindings through it. Do not rely on a system-wide LLVM install — download a current LLVM release from [github.com/llvm/llvm-project/releases](https://github.com/llvm/llvm-project/releases) (e.g. `llvmorg-17.0.6` for Windows) and extract it into the repository-local `.tools/` folder.

`whisper-rs-sys` currently builds with bindgen 0.69, which mis-generates opaque structs with libclang from LLVM 18 and newer (see the whisper.cpp `whisper_full_params` bindings). Use LLVM 17.x for the repository-local toolchain until `whisper-rs-sys` moves to bindgen 0.70+; `clang-sys` (the `bindgen` → libclang bridge) does not enforce an upper Clang version, and the libclang C API is backward compatible.

Point the build at the extracted toolchain:

```bash
# ~/.bashrc / ~/.zshrc — adjust to the actual extracted path
export LLVM_CONFIG_PATH="$PWD/.tools/llvm/bin/llvm-config"
export LIBCLANG_PATH="$PWD/.tools/llvm/lib"
```

On Windows (PowerShell, e.g. in `build.bat`): `set "LIBCLANG_PATH=<repo>\.tools\llvm\bin"`.

After staging the helper, run application commands from `frontend/` with dependencies installed:

| Command | Purpose |
| --- | --- |
| `pnpm run dev` | Next.js UI development server at `http://localhost:3118`; native Tauri behavior requires the desktop app |
| `pnpm run tauri:dev` | Full Tauri development app with build-time acceleration detection |
| `pnpm run tauri:build` | Desktop production build |
| `pnpm run tauri:dev:cpu` | Development app without an explicit GPU feature flag |
| `pnpm run tauri:dev:metal` | Development app with Metal |
| `pnpm run tauri:dev:cuda` | Development app with CUDA |
| `pnpm run tauri:dev:vulkan` | Development app with Vulkan |

The feature-specific commands require a compatible platform and toolchain. The maintained command definitions are in [package.json](../frontend/package.json); see the [GPU acceleration guide](GPU_ACCELERATION.md) for setup.

Development builds already log `app_lib` at Debug level, normally to both the terminal and `logs-dev/` below the app data directory. No environment variable is needed for `pnpm run tauri:dev`.

Release builds normally log `app_lib` at Info level under `logs/`. To enable Debug logging, launch the installed executable with `SAMOBRANKA_DEBUG_LOG=1` on macOS/Linux. In Windows PowerShell, set `$env:SAMOBRANKA_DEBUG_LOG="1"` before launching the installed executable. This does not restore Trace messages compiled out of release builds.

| Platform | App data directory |
| --- | --- |
| Linux | `${XDG_DATA_HOME:-$HOME/.local/share}/ai.samobranka.app` |
| macOS | `~/Library/Application Support/ai.samobranka.app` |
| Windows | `%APPDATA%\ai.samobranka.app` |

Append `logs-dev/` for development builds or `logs/` for release builds. If the persistent log path is unavailable or unwritable, logging may fall back to a stream, so log files are not guaranteed.

`RUST_LOG` does not control Samobranka's configured application logger; build tools or dependencies may handle it separately.

Inspect the terminal's Rust logs and the desktop app's developer tools when diagnosing frontend/native interaction. For audio problems, inspect the pipeline and transcription-worker summaries for dropped samples, speech-segment statistics, queue depth, and failures. Use synthetic audio and redact sensitive data before sharing diagnostics.

The `clean_run.sh` and Windows wrapper scripts are alternative launch paths, but inspect them before use: clean-run scripts can remove frontend dependencies and generated output. After sidecar preparation, prefer the commands above when you do not need a clean rebuild.


<details>
<summary>Linux</summary>

## 🐧 Building on Linux

This guide helps you build Samobranka on Linux with **automatic GPU acceleration**. The build system detects your hardware and configures the best performance automatically.

---

### 🚀 Quick Start (Recommended for Beginners)

If you're new to building on Linux, start here. These simple commands work for most users:

#### 1. Install Basic Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git

# Arch Linux
sudo pacman -S base-devel cmake git
```

#### 2. Build and Run

```bash
# Development mode (with hot reload)
./dev-gpu.sh

# Production build
./build-gpu.sh
```

**That's it!** The scripts automatically detect your GPU and configure acceleration.

### What Happens Automatically?

- ✅ **NVIDIA GPU** → CUDA acceleration (if toolkit installed)
- ✅ **AMD GPU** → ROCm acceleration (if ROCm installed)
- ✅ **No GPU** → Optimized CPU mode (still works great!)

> 💡 **Tip:** If you have an NVIDIA or AMD GPU but want better performance, jump to the [GPU Setup](#-gpu-setup-guides-intermediate) section below.

---

### 🧠 Understanding Auto-Detection

The build scripts (`dev-gpu.sh` and `build-gpu.sh`) orchestrate the entire build process. Here's how they work:

1.  **Detect location:** Find `package.json` (works from project root or `frontend/`)
2.  **Auto-detect GPU:** Run `scripts/auto-detect-gpu.js` (or use `TAURI_GPU_FEATURE` if set)
3.  **Build Sidecar:** Build `llama-helper` with the detected feature (debug or release)
4.  **Copy Binary:** Copy the built sidecar to `src-tauri/binaries` with the target triple
5.  **Run Tauri:** Call `npm run tauri:dev` or `tauri:build` with the feature flag passed via env var

#### Detection Priority

| Priority | Hardware        | What It Checks                                               | Result                  |
| -------- | --------------- | ------------------------------------------------------------ | ----------------------- |
| 1️⃣       | **NVIDIA CUDA** | `nvidia-smi` exists + (`CUDA_PATH` or `nvcc` found)          | `--features cuda`       |
| 2️⃣       | **AMD ROCm**    | `rocm-smi` exists + (`ROCM_PATH` or `hipcc` found)           | `--features hipblas`    |
| 3️⃣       | **Vulkan**      | `vulkaninfo` exists + `VULKAN_SDK` + `BLAS_INCLUDE_DIRS` set | `--features vulkan`     |
| 4️⃣       | **OpenBLAS**    | `BLAS_INCLUDE_DIRS` set                                      | `--features openblas`   |
| 5️⃣       | **CPU-only**    | None of the above                                            | (no features, pure CPU) |

#### Common Scenarios

| Your System               | Auto-Detection Result       | Why                          |
| ------------------------- | --------------------------- | ---------------------------- |
| Clean Linux install       | CPU-only                    | No GPU SDK detected          |
| NVIDIA GPU + drivers only | CPU-only                    | CUDA toolkit not installed   |
| NVIDIA GPU + CUDA toolkit | **CUDA acceleration** ✅    | Full detection successful    |
| AMD GPU + ROCm            | **HIPBlas acceleration** ✅ | Full detection successful    |
| Vulkan drivers only       | CPU-only                    | Vulkan SDK + env vars needed |
| Vulkan SDK configured     | **Vulkan acceleration** ✅  | All requirements met         |

> 💡 **Key Insight:** Having GPU drivers alone isn't enough. You need the **development SDK** (CUDA toolkit, ROCm, or Vulkan SDK) for acceleration.

---

### 🔧 GPU Setup Guides (Intermediate)

Want better performance? Follow these guides to enable GPU acceleration.

#### 🟢 NVIDIA CUDA Setup

**Prerequisites:** NVIDIA GPU with compute capability 5.0+ (check: `nvidia-smi --query-gpu=compute_cap --format=csv`)

##### Step 1: Install CUDA Toolkit

```bash
# Ubuntu/Debian (CUDA 12.x)
sudo apt install nvidia-driver-550 nvidia-cuda-toolkit

# Verify installation
nvidia-smi          # Shows GPU info
nvcc --version      # Shows CUDA version
```

##### Step 2: Build with CUDA

```bash
# Set your GPU's compute capability
# Example: RTX 3080 = 8.6 → use "86"
# Example: GTX 1080 = 6.1 → use "61"

CMAKE_CUDA_ARCHITECTURES=75 \
CMAKE_CUDA_STANDARD=17 \
CMAKE_POSITION_INDEPENDENT_CODE=ON \
./build-gpu.sh
```

> 💡 **Finding Your Compute Capability:**
>
> ```bash
> nvidia-smi --query-gpu=compute_cap --format=csv
> ```
>
> Convert `7.5` → `75`, `8.6` → `86`, etc.

**Why these flags?**

- `CMAKE_CUDA_ARCHITECTURES`: Optimizes for your specific GPU
- `CMAKE_CUDA_STANDARD=17`: Ensures C++17 compatibility
- `CMAKE_POSITION_INDEPENDENT_CODE=ON`: Fixes linking issues on modern systems

---

#### 🔵 Vulkan Setup (Cross-Platform Fallback)

Vulkan works on NVIDIA, AMD, and Intel GPUs. Good choice if CUDA/ROCm don't work.

> **Requirement:** the latest Vulkan SDK must be downloaded into the repository-local `.tools/` folder (do not rely on a system-wide SDK install). Get it from https://vulkan.lunarg.com/sdk/home and extract the archive so that the SDK root (containing `include/`, `lib/`, `bin/`) lives under `.tools/`, for example `.tools/vulkan-sdk/...`. The folder is git-ignored.

##### Step 1: Download the Vulkan SDK

Download the latest SDK for your platform from [vulkan.lunarg.com/sdk/home](https://vulkan.lunarg.com/sdk/home) and extract it into `.tools/` inside the repository root.

OpenBLAS still comes from the distro:

```bash
# Ubuntu/Debian
sudo apt install libopenblas-dev

# Fedora
sudo dnf install openblas-devel

# Arch Linux
sudo pacman -S openblas
```

##### Step 2: Configure Environment

Point `VULKAN_SDK` at the SDK extracted into `.tools/`:

```bash
# Add to ~/.bashrc or ~/.zshrc
export VULKAN_SDK="$PWD/.tools/vulkan-sdk"   # adjust to the actual extracted path
export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu

# Apply changes
source ~/.bashrc
```

##### Step 3: Build

```bash
./build-gpu.sh
```

The script will automatically detect Vulkan and build with `--features vulkan`.

---

#### 🔴 AMD ROCm Setup (AMD GPUs Only)

**Prerequisites:** AMD GPU with ROCm support (RX 5000+, Radeon VII, etc.)

```bash
# Ubuntu/Debian
# Add ROCm repository (see https://rocm.docs.amd.com for latest)
sudo apt install rocm-smi hipcc

# Set environment
export ROCM_PATH=/opt/rocm

# Verify
rocm-smi            # Shows GPU info
hipcc --version     # Shows ROCm version

# Build
./build-gpu.sh
```

---

### 🎯 Advanced Usage

#### Manual Feature Override

Want to force a specific acceleration method? Use the `TAURI_GPU_FEATURE` environment variable with the shell scripts:

```bash
# Force CUDA (ignore auto-detection)
TAURI_GPU_FEATURE=cuda ./dev-gpu.sh
TAURI_GPU_FEATURE=cuda ./build-gpu.sh

# Force Vulkan
TAURI_GPU_FEATURE=vulkan ./dev-gpu.sh
TAURI_GPU_FEATURE=vulkan ./build-gpu.sh

# Force ROCm (HIPBlas)
TAURI_GPU_FEATURE=hipblas ./dev-gpu.sh
TAURI_GPU_FEATURE=hipblas ./build-gpu.sh

# Force CPU-only (for testing)
TAURI_GPU_FEATURE="" ./dev-gpu.sh
TAURI_GPU_FEATURE="" ./build-gpu.sh

# Force OpenBLAS (CPU-optimized)
TAURI_GPU_FEATURE=openblas ./dev-gpu.sh
TAURI_GPU_FEATURE=openblas ./build-gpu.sh
```

#### Build Output Location

After successful build:

```
src-tauri/target/release/bundle/appimage/Samobranka_<version>_amd64.AppImage
```

---

### 🧭 Troubleshooting

#### "CUDA toolkit not found"

- **Fix:** Install `nvidia-cuda-toolkit` or set `CUDA_PATH` environment variable
- **Check:** `nvcc --version` should work

#### "Vulkan detected but missing dependencies"

- **Fix:** Set both `VULKAN_SDK` and `BLAS_INCLUDE_DIRS` environment variables
- **Example:**
  ```bash
  export VULKAN_SDK=/usr
  export BLAS_INCLUDE_DIRS=/usr/include/x86_64-linux-gnu
  ```

#### "AppImage build stripping symbols"

- **Fix:** Already handled! `build-gpu.sh` sets `NO_STRIP=true` automatically
- **Why:** Prevents runtime errors from missing symbols

#### Build works but no GPU acceleration

- **Check detection:** Look at the build output for GPU detection messages
- **Verify:** `nvidia-smi` (NVIDIA) or `rocm-smi` (AMD) should work
- **Missing SDK:** Install the development toolkit, not just drivers

</details>

<details>
<summary>macOS</summary>

## 🍎 Building on macOS

On macOS, the build process is simplified as GPU acceleration (Metal) is enabled by default.

### 1. Install Dependencies

```bash
# Install Homebrew (if not already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install required tools
brew install cmake node pnpm
```

### 2. Build and Run

```bash
# Development mode (with hot reload)
pnpm tauri:dev

# Production build
pnpm tauri:build
```

The application will be built with Metal GPU acceleration automatically.

</details>

<details>
<summary>Windows</summary>

## 🪟 Building on Windows

### 1. Install Dependencies

- **Node.js:** Download and install from [nodejs.org](https://nodejs.org/).
- **Rust:** Install from [rust-lang.org](https://www.rust-lang.org/tools/install).
- **pnpm:** Install version 9.15.9 with `npm install -g pnpm@9.15.9`.
- **Visual Studio Build Tools:** Install the "Desktop development with C++" workload from the Visual Studio Installer.
- **CMake:** Download and install from [cmake.org](https://cmake.org/download/).

### 2. Build and Run

```powershell
pnpm install --frozen-lockfile
```

```powershell
# Development mode (with hot reload)
pnpm tauri:dev

# Production build
pnpm tauri:build
```

By default, the application will be built with CPU-only processing. To enable GPU acceleration, see the [GPU Acceleration Guide](GPU_ACCELERATION.md).

### Windows Distribution Builds

The commands above create a local source build. Use the production Windows build workflow for an installer intended for other computers: it enables Vulkan. Rust targets `x86-64-v2`; native Whisper retains AVX2 with host-native specialization and AVX-512 disabled.

The distribution workflows (`build.yml`, `build-windows.yml`, and `build-devtest.yml`) set `CMAKE_PROJECT_INCLUDE` to `.github/force-portable-ggml.cmake`, which forces `GGML_NATIVE=OFF`, and use `RUSTFLAGS=-C target-cpu=x86-64-v2`. The hook configures Whisper's native C/C++ build; Rust flags do not. `WHISPER_NATIVE=OFF` and plain `GGML_*` variables are not replacements.

The Windows distribution workflows also stage the audioCpp provider runtime before bundling: `frontend/src-tauri/scripts/stage-audiocpp-runtime.ps1` downloads the pinned audio.cpp v0.9.5 lib archive (SHA256-verified) and copies `audiocpp.dll`, the MSVC runtime bundle (`msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll`), and the Apache-2.0 license into `frontend/src-tauri/audiocpp-runtime/`. `tauri.windows.conf.json` maps that directory into the install root, next to the executable, so the installed app runs without the Visual C++ Redistributable. For a local development build, point `AUDIOCPP_LIBRARY_PATH` at `audiocpp.dll` instead (see `audio_cpp_engine/wrapper.rs`).

The transcribeCpp provider runtime is staged with the cross-platform script `frontend/scripts/stage-transcribe-native.mjs` (`pnpm run stage:transcribe` in `frontend/`, and a CI step in `build.yml`/`build-windows.yml` before bundling). It downloads the transcribe.cpp v0.4.2 release archive for the current lane (`linux-x86_64` / `linux-aarch64` / `windows-x86_64` — cpu-vulkan, `macos-arm64` — metal), verifies its pinned SHA-256, and extracts `libtranscribe.*` + ggml backend modules + `contract.json` + `licenses/` flat into `frontend/src-tauri/transcribecpp-runtime/` (gitignored; re-runs are skipped via a stamp file). `tauri.windows.conf.json` maps that directory into the Windows install root. Unlike the audio.cpp bundle, no MSVC redist DLLs are staged: the upstream v0.4.2 Windows wheel is repaired with `delvewheel repair --exclude vulkan-1.dll` and delvewheel never vendors `vcruntime140*`/`msvcp*`/`vcomp*`, so the Visual C++ 2015-2022 Redistributable and the Vulkan loader (`vulkan-1.dll`, installed with GPU drivers) are expected from the system.

</details>
