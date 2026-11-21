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

- **Linux:** Samobranka is built from source; the standard `pnpm run tauri:build` produces the distribution packages (the transcribe.cpp runtime is staged separately, see below).
- **Windows packages:** Distribution builds target a portable x64 baseline and require an AVX2-capable x64 CPU. AVX-512 is not required.
- **GPU acceleration:** there are no compile-time GPU build variants. Transcription runs on the prebuilt transcribe.cpp runtime, which uses Vulkan through the system loader at runtime and falls back to CPU; see the [GPU acceleration guide](GPU_ACCELERATION.md).

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

These commands build the helper without an explicit GPU backend. Its optional acceleration features are `metal`, `cuda`, and `vulkan`, each requiring a compatible platform and toolchain.

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

**Vulkan note:** staging itself needs no SDK — the release lanes ship prebuilt Vulkan backend modules. Running transcribe.cpp on Vulkan only needs a working Vulkan driver/loader at runtime (`libvulkan.so.1` on Linux, `vulkan-1.dll` on Windows, installed with GPU drivers). Without a usable driver the engine falls back to CPU with a user-facing warning; see [RUNNING_LINUX.md](RUNNING_LINUX.md) / [RUNNING_WINDOWS.md](RUNNING_WINDOWS.md). The only compile-time Vulkan build left in the workspace is the optional `vulkan` feature of the `llama-helper` sidecar (summary inference); it is CPU-only in the shipped Windows and Linux release lanes and requires a Vulkan SDK only if you build it with that feature.

### Local LLVM toolchain (`.tools/`)

The build requires **libclang** at compile time: `bindgen` (pulled in by `llama-cpp-sys-2` for the `llama-helper` sidecar) generates FFI bindings through it. Do not rely on a system-wide LLVM install — download a current LLVM release from [github.com/llvm/llvm-project/releases](https://github.com/llvm/llvm-project/releases) and extract it into the repository-local `.tools/` folder.

Point the build at the extracted toolchain:

```bash
# ~/.bashrc / ~/.zshrc — adjust to the actual extracted path
export LLVM_CONFIG_PATH="$PWD/.tools/llvm/bin/llvm-config"
export LIBCLANG_PATH="$PWD/.tools/llvm/lib"
```

On Windows (PowerShell): `$env:LIBCLANG_PATH = "<repo>\.tools\llvm\bin"`.

After staging the helper and the transcribe.cpp runtime, run application commands from `frontend/` with dependencies installed:

| Command | Purpose |
| --- | --- |
| `pnpm run dev` | Next.js UI development server at `http://localhost:3118`; native Tauri behavior requires the desktop app |
| `pnpm run tauri:dev` | Full Tauri development app |
| `pnpm run tauri:build` | Desktop production build |
| `pnpm run stage:transcribe` | Stage the transcribe.cpp native runtime (see above) |

There are no GPU build variants to select: the maintained command definitions are in [package.json](../frontend/package.json), and runtime acceleration is covered by the [GPU acceleration guide](GPU_ACCELERATION.md).

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

Samobranka builds from source on x86_64 Linux with the standard commands. There are no compile-time GPU build variants to choose: the app itself compiles CPU-only, transcription acceleration is a runtime decision of the transcribe.cpp engine (Auto / Vulkan / CPU), and the optional GPU features of the `llama-helper` sidecar (`metal`, `cuda`, `vulkan`) only apply if you intentionally build it that way.

### 1. Install Basic Dependencies

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install build-essential cmake git

# Fedora/RHEL
sudo dnf install gcc-c++ cmake git

# Arch Linux
sudo pacman -S base-devel cmake git
```

For the full dependency list (audio capture, Tauri webview, libclang for the sidecar) see [RUNNING_LINUX.md](RUNNING_LINUX.md).

### 2. Prepare the Native Pieces

From the repository root, after the [first-run sidecar setup](#first-run-sidecar-setup), stage the transcribe.cpp runtime from `frontend/`:

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm run stage:transcribe
```

### 3. Build and Run

```bash
# Development mode (with hot reload)
pnpm run tauri:dev

# Production build (deb + AppImage)
pnpm run tauri:build
```

Build output:

```
target/release/bundle/appimage/Samobranka_<version>_amd64.AppImage
target/release/bundle/deb/Samobranka_<version>_amd64.deb
```

### Runtime Vulkan

The staged transcribe.cpp lanes ship prebuilt Vulkan backend modules; running on GPU needs only a working Vulkan driver/loader (`libvulkan.so.1`, installed with GPU drivers — check with `vulkaninfo`). No SDK, `VULKAN_SDK`, or BLAS environment variables are required to build or run the app. Without a usable driver the engine falls back to CPU with a user-facing warning. See [RUNNING_LINUX.md](RUNNING_LINUX.md) for diagnostics and the transcribe.cpp staging details.

</details>

<details>
<summary>macOS</summary>

## 🍎 Building on macOS

On macOS, the build is the standard `pnpm` flow; the app compiles CPU-only and GPU acceleration happens at runtime (transcribe.cpp `macos-arm64-metal` lane).

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

The app itself compiles CPU-only. Transcription acceleration comes from the staged transcribe.cpp runtime (`macos-arm64-metal` lane, Metal backend at runtime), and the CI builds the `llama-helper` sidecar with its optional `metal` feature.

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

The application builds CPU-only; there is no compile-time GPU flag to enable. Transcription GPU acceleration is runtime Vulkan through the system loader — see the [GPU Acceleration Guide](GPU_ACCELERATION.md).

### Windows Distribution Builds

The commands above create a local source build. Use the production Windows build workflow for an installer intended for other computers: Rust targets `x86-64-v2`, so the installer retains an AVX2-capable baseline with host-native specialization and AVX-512 disabled.

The distribution workflows (`build.yml`, `build-windows.yml`, and `build-devtest.yml`) set `CMAKE_PROJECT_INCLUDE` to `.github/force-portable-ggml.cmake`, which forces `GGML_NATIVE=OFF`, and use `RUSTFLAGS=-C target-cpu=x86-64-v2`. The hook configures the native C/C++ build of the `llama-helper` sidecar (llama.cpp via `llama-cpp-sys-2`) — the only natively-compiled dependency shipped in the app; Rust flags do not configure it. Plain `GGML_*` environment variables are not a replacement. The same workflows run `verify-portable-ggml.cjs` before bundling to fail the build if the sidecar's CMake cache shows host-native specialization or AVX-512.

The Windows distribution workflows also stage the audio.cpp diarization runtime before bundling: `frontend/src-tauri/scripts/stage-audiocpp-runtime.ps1` downloads the pinned audio.cpp v0.9.5 lib archive (SHA256-verified) and copies `audiocpp.dll`, the MSVC runtime bundle (`msvcp140*.dll`, `vcruntime140*.dll`, `vcomp140.dll`), and the Apache-2.0 license into `frontend/src-tauri/audiocpp-runtime/`. `tauri.windows.conf.json` maps that directory into the install root, next to the executable, so the installed app runs without the Visual C++ Redistributable. For a local development build, point `AUDIOCPP_LIBRARY_PATH` at `audiocpp.dll` instead (see `audio_cpp_engine/wrapper.rs`).

The transcribeCpp engine's runtime is staged with the cross-platform script `frontend/scripts/stage-transcribe-native.mjs` (`pnpm run stage:transcribe` in `frontend/`, and CI staging/verify steps in `build.yml` for Linux/macOS and `build-windows.yml` for Windows before bundling). It downloads the transcribe.cpp v0.4.2 release archive for the current lane (`linux-x86_64` / `linux-aarch64` / `windows-x86_64` — cpu-vulkan, `macos-arm64` — metal), verifies its pinned SHA-256, and extracts `libtranscribe.*` + ggml backend modules + `contract.json` + `licenses/` flat into `frontend/src-tauri/transcribecpp-runtime/` (gitignored; re-runs are skipped via a stamp file). `tauri.windows.conf.json` maps that directory into the Windows install root. Unlike the audio.cpp bundle, no MSVC redist DLLs are staged: the upstream v0.4.2 Windows wheel is repaired with `delvewheel repair --exclude vulkan-1.dll` and delvewheel never vendors `vcruntime140*`/`msvcp*`/`vcomp*`, so the Visual C++ 2015-2022 Redistributable and the Vulkan loader (`vulkan-1.dll`, installed with GPU drivers) are expected from the system.

</details>
