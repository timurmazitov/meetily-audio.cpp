# GPU Acceleration Guide

Samobranka has no compile-time GPU build variants. The application always compiles CPU-only; hardware acceleration is chosen at runtime by the native runtimes it loads:

*   **Transcription** runs on the prebuilt transcribe.cpp runtime (staged with `pnpm run stage:transcribe`) and follows the compute-mode setting in **Settings → Transcript Model**: **Auto**, **Vulkan (GPU)**, or **CPU**.
*   **Diarization** runs on the audio.cpp library (Nemotron-3 Diarization model) and picks its backend automatically from the available hardware.
*   **Summaries** via the local `llama-helper` sidecar can use its optional GPU features (`metal`, `cuda`, `vulkan`); the shipped release lanes build it CPU-only on Windows/Linux and with Metal on macOS. See the [build instructions](BUILDING.md#first-run-sidecar-setup).

## transcribe.cpp (transcription)

The staged release lanes ship prebuilt Vulkan backend modules next to the CPU ones. No SDK or toolkit is needed at build time: running on GPU only requires a working Vulkan driver/loader provided by the system (`libvulkan.so.1` on Linux, `vulkan-1.dll` on Windows — both installed with GPU drivers).

Behavior:

*   **Vulkan requested but no usable driver** — the engine falls back to CPU automatically with a user-facing warning (`transcription-warning`) and a log entry; transcripts are unaffected, just slower.
*   **The backend mask is fixed per process.** The transcribe.cpp library fixes the set of allowed backends at the first registration, so changing Auto/Vulkan/CPU takes effect only after restarting the app.
*   On macOS the staged lane carries Metal backend modules instead of Vulkan.

Verify a Vulkan driver without the app: `vulkaninfo` (Linux/Windows SDK tools) or the `transcribecpp_diagnostics` Tauri command, which reports the loaded library, version, ABI hash, and the lane's backend list.

## Diarization (audio.cpp)

The audio.cpp diarization library is loaded at runtime and its compute backend is chosen automatically from hardware detection:

*   **NVIDIA GPU detected** — the engine runs on GPU through the Vulkan variant of the audio.cpp library (Vulkan is served by the NVIDIA driver ICD). No manual configuration is needed.
*   **AMD/Intel GPU detected** — the Vulkan variant is used.
*   **No supported GPU** — the CPU variant is used and the app keeps working; diarization is just slower.

There is **no CUDA codepath** for audio.cpp. The C ABI accepts a `"cuda"` backend token, but the pinned audio.cpp release archives ship only `cpu` and `vulkan` library flavours, so this app never selects or loads a CUDA library. NVIDIA hardware is accelerated through Vulkan; that is the supported path.

Packaging places each shipped flavour next to the executable as
`audiocpp-libs/cpu/` and `audiocpp-libs/vulkan/`; the loader picks the
directory matching the autodetected backend (single-variant and development
installs keep working via the flat layout, and the `AUDIOCPP_LIBRARY_PATH`
environment variable still overrides everything).

Downloaded models are backend-agnostic GGUF files: every library variant
loads the same catalog, so switching the compute backend (or moving a model
directory between machines) never invalidates an already-downloaded model or
any setting.

## Platform Notes

### Linux

Nothing to install for GPU acceleration: the Vulkan loader comes from the system with the GPU drivers. The transcribe.cpp runtime is staged with `pnpm run stage:transcribe` (see [RUNNING_LINUX.md](RUNNING_LINUX.md)). The optional `vulkan` feature of the `llama-helper` sidecar is the only build that needs a Vulkan SDK, and only if you choose to build it that way.

### macOS

The transcribe.cpp `macos-arm64-metal` lane provides Metal acceleration at runtime; no configuration is required.

### Windows

GPU drivers must provide `vulkan-1.dll` (the system Vulkan loader). It is never vendored into the installer; the transcribe.cpp runtime resolves it at execution. See [RUNNING_WINDOWS.md](RUNNING_WINDOWS.md) for staging and the VC++ Redistributable requirement.
