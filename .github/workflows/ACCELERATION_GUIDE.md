# CI/CD Hardware Acceleration Guide

This document explains the hardware acceleration and CPU-portability configuration used by Samobranka CI/CD workflows.

## Overview

The application itself compiles CPU-only on every platform: there are no compile-time GPU feature flags in the Tauri build. Acceleration comes from the native runtimes the app loads at execution:

| Component | Acceleration | How it is chosen | Distribution baseline |
| --- | --- | --- | --- |
| **Transcription (transcribe.cpp)** | Vulkan or CPU (Metal on macOS) | Staged prebuilt runtime; compute mode Auto / Vulkan / CPU in settings; driver-provided system Vulkan loader | AVX2-capable x64 CPU; AVX-512 disabled |
| **Diarization (audio.cpp)** | Vulkan or CPU | Loaded at runtime, backend autodetected; packaged as `audiocpp-libs/cpu|vulkan` | — |
| **Summaries (llama-helper sidecar)** | CPU; optional `metal`/`cuda`/`vulkan` Cargo features | Built per platform in CI (see below) | — |

No workflow installs a Vulkan SDK for the Tauri app, and `vulkan-1.dll` / `libvulkan.so` are never vendored: the transcribe.cpp runtime resolves the Vulkan loader from the system (installed with GPU drivers). Without a usable driver the engine falls back to CPU with a user-facing warning. CUDA is not a CI configuration; see [docs/GPU_ACCELERATION.md](../../docs/GPU_ACCELERATION.md) for the runtime model.

The `llama-helper` sidecar (local summary inference via llama.cpp) is the only natively-compiled dependency shipped in the app. Its GPU features are optional and CI enables them selectively:

| Workflow | Platform | llama-helper build |
| --- | --- | --- |
| `build.yml` (shared release) | Linux | `cargo build --release -p llama-helper --features vulkan` (needs the prepared Vulkan SDK) |
| `build.yml` (shared release) | Windows | CPU-only (Vulkan build has CMake race issues in `llama-cpp-sys-2`) |
| `build.yml` (shared release) | macOS | `--features metal` |
| `build-devtest.yml` | Windows | `--features vulkan` (workflow installs Vulkan SDK `1.4.309.0` for this compile-time need) |
| `build-devtest.yml` | Linux | CPU-only (no Vulkan SDK) |
| `build-windows.yml` | Windows | CPU-only |
| `build-linux.yml` | Linux | CPU-only (no Vulkan SDK / OpenBLAS install) |

## Windows Package Portability

A release installer is intentionally different from a build optimized for one local machine:

- Rust targets `x86-64-v2`.
- The llama-helper sidecar keeps the portable ggml baseline: host-native specialization off, AVX2 retained.

The workflows set `CMAKE_PROJECT_INCLUDE` to `force-portable-ggml.cmake`, which forces:

```cmake
set(GGML_NATIVE OFF CACHE BOOL "Build without host CPU specialization" FORCE)
```

Rust target flags do not configure the sidecar's native C/C++ compilation. Plain `GGML_*` environment variables are not a replacement for the hook.

### Portable Cache and Pre-Bundle Verification

Windows portable builds use the `windows-portable-v1` Rust-cache prefix, preventing an earlier host-native target cache from being restored through a fallback key (Linux uses `linux-portable-v1`).

Before bundling, `verify-portable-ggml.cjs` reads the llama-helper sidecar's ggml CMake cache (under `build/llama-cpp-sys-2-*/out/build`). It fails the build if the cache is missing, `GGML_NATIVE` or any AVX-512 option is enabled, or a host-specific `-march/-mcpu/-mtune=native` flag is present. This prevents an unverified native library from becoming an installer asset.

## Native Runtime Staging

Instead of compiling GPU backends, the workflows stage prebuilt native runtimes before bundling:

- **transcribe.cpp** — `node scripts/stage-transcribe-native.mjs` (from `frontend/`) downloads the pinned v0.4.2 lane for the platform (`linux-x86_64-cpu-vulkan`, `windows-x86_64-cpu-vulkan`, `macos-arm64-metal`), verifies the SHA-256 pin, and extracts into `frontend/src-tauri/transcribecpp-runtime/`. A `--verify` step asserts completeness. `tauri.windows.conf.json` maps the directory into the Windows install root. The shared `build.yml` runs these steps on Linux and macOS; the Windows lanes are staged by the standalone `build-windows.yml`.
- **audio.cpp (diarization)** — on Windows, `frontend/src-tauri/scripts/stage-audiocpp-runtime.ps1` downloads the pinned v0.9.5 vulkan lib archive (SHA256-verified) and stages `audiocpp.dll` plus the MSVC runtime bundle into `frontend/src-tauri/audiocpp-runtime/`.

## Updated Workflows

### 1. `build.yml` (Shared Release Workflow)

When building Windows, the shared workflow:

- Applies the portable CMake hook and `x86-64-v2` Rust target.
- Uses the `windows-portable-v1` cache prefix.
- Runs the pre-bundle llama-helper portability verification.
- Builds `llama-helper` CPU-only; stages the transcribe.cpp and audio.cpp runtimes.

For Linux builds it prepares the Vulkan SDK (`1.4.363.0`, downloaded and hash-pinned in the workflow) because the `llama-helper` sidecar builds with `--features vulkan` there; the Tauri app itself compiles CPU-only and the transcribe.cpp runtime is staged prebuilt. On Windows the shared workflow stages the audio.cpp runtime; on Linux/macOS it stages the transcribe.cpp runtime (`windows-x86_64` lanes are staged by the standalone `build-windows.yml`, whose install-root mapping comes from `tauri.windows.conf.json`).

### 2. `build-devtest.yml` (DevTest)

The Windows DevTest path applies the same portable CMake hook, Rust target, cache prefix, and pre-bundle verification as a release build, and installs Vulkan SDK `1.4.309.0` because its `llama-helper` build uses `--features vulkan`. The Linux path builds `llama-helper` CPU-only and installs no Vulkan SDK. Keep those safeguards in place when changing DevTest behavior.

### 3. `build-windows.yml` / `build-linux.yml` (Standalone)

The standalone workflows apply the same portability safeguards before packaging and build `llama-helper` CPU-only. A change is incomplete if it updates only the shared workflow or only a standalone workflow.

## Verification

### How to Verify a Windows Package Build

1. **Check the workflow configuration**
   - `CMAKE_PROJECT_INCLUDE` uses the portable hook.
   - Rust uses `-C target-cpu=x86-64-v2`.
   - No Vulkan SDK or GPU feature flags for the Tauri app build itself.

2. **Check native sidecar configuration**
   - `GGML_NATIVE` is OFF; AVX-512 options are OFF.
   - The `verify-portable-ggml.cjs` command completes before bundling.

3. **Check cache isolation**
   - Windows builds use the `windows-portable-v1` cache prefix.
   - Do not reuse a native cache whose CPU configuration is unknown.

4. **Check staged runtimes**
   - transcribe.cpp staging + `--verify` steps completed.
   - On Windows, the audio.cpp runtime verify step found `audiocpp.dll` and the MSVC bundle.

### Runtime Verification

Confirm the packaged application runs on an AVX2-capable Windows computer without AVX-512, transcribes on the staged runtime, and falls back to CPU with a warning where no Vulkan driver is present.

## Troubleshooting

### A llama-helper Vulkan Build Fails

- Confirm the Vulkan SDK install/validation step completed (DevTest Windows, shared Linux release).
- pnpm installation does not install or repair the Vulkan SDK.
- The standalone Windows workflow intentionally builds the sidecar CPU-only (CMake race condition in `llama-cpp-sys-2` Vulkan builds).

### Portability Verification Fails

Treat the failure as a packaging blocker. Inspect the generated llama-helper CMake cache and the portable cache prefix; do not bypass the pre-bundle verifier. A fresh portable build is safer than reusing a cache with unknown CPU flags.

### Transcription Runs on CPU

No usable Vulkan driver/loader on the machine — the engine falls back to CPU with a user-facing warning. Check `vulkaninfo` and the compute-mode setting (Auto / Vulkan / CPU); changing the mode requires an app restart because the library fixes the backend mask per process.

## Related Documentation

- [Building from Source](../../docs/BUILDING.md)
- [GPU Acceleration Guide](../../docs/GPU_ACCELERATION.md)
- [Workflow Overview](WORKFLOWS_OVERVIEW.md)
- [DevTest Workflow Guide](README_DEVTEST.md)
- [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp)

## Summary

- The Tauri app always compiles CPU-only; acceleration is runtime Vulkan (transcribe.cpp / audio.cpp) through the system loader, with automatic CPU fallback.
- Windows packages retain the portable x86-64-v2 / AVX2 baseline via the portable CMake hook, Rust target, cache prefix, and pre-bundle verification of the llama-helper sidecar — preserve them as a unit.
- Vulkan SDK appears in CI only where the `llama-helper` sidecar compiles with its `vulkan` feature.
- CUDA remains an out-of-CI, local source-build choice for `llama-helper`.
