# Agent Guide for Samobranka

These instructions apply to AI coding agents working in this repository.

## Project Overview

- Samobranka is a desktop application for recording meetings, transcribing audio, and generating summaries.
- The supported app lives in `frontend/`: Next.js/React provides the UI, and the Rust/Tauri core handles native behavior, audio, persistence, transcription, and summary orchestration.
- Local transcription uses one of three engines: Whisper, Parakeet, or audioCpp (audio.cpp / GigaAM). Summaries use the configured local or cloud provider; do not assume every provider runs locally.
- Frontend code invokes Tauri commands; Rust emits events that update frontend state. Preserve existing command contracts, event payloads, and state ownership.
- `backend/` is an unsupported legacy archive, not a runtime dependency or a place to add new application behavior. Its FastAPI/CORS behavior is not a supported production API.

See [architecture and audio pipeline](docs/architecture.md), including [frontend/native communication](docs/architecture.md#frontend-and-native-communication) and [model management](docs/architecture.md#transcription-and-model-management).

## Agent skills

### Issue tracker

Issues and specs live as markdown files under `.scratch/` in this repo. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: the five canonical labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Development Commands

Use pnpm 9.15.9. Run these commands from `frontend/`:

Before running desktop dev/build commands, complete the [first-run sidecar setup](docs/BUILDING.md#first-run-sidecar-setup). Installing frontend dependencies alone does not stage `llama-helper`.

Vulkan builds additionally require the latest Vulkan SDK downloaded into the repository-local `.tools/` folder (not a system-wide install). See [Vulkan setup](docs/BUILDING.md#-vulkan-setup-cross-platform-fallback).

All builds require libclang for `bindgen`; download a current LLVM release into `.tools/` and set `LLVM_CONFIG_PATH`/`LIBCLANG_PATH`. See [local LLVM toolchain](docs/BUILDING.md#local-llvm-toolchain-tools).

| Command | Purpose |
| --- | --- |
| `pnpm install --frozen-lockfile` | Install the committed dependency set |
| `pnpm run tauri:dev` | Run the full desktop app |
| `pnpm run tauri:build` | Build the desktop app for production |

Use the Tauri app for development and validation; a standalone Next.js server does not exercise native commands, audio, or permissions.

See [build and platform setup](docs/BUILDING.md), [development commands and debugging](docs/BUILDING.md#development-commands-and-debugging), and [GPU acceleration](docs/GPU_ACCELERATION.md) for prerequisites and platform-specific options. After sidecar preparation, prefer the package scripts over clean-run wrappers, which can remove dependencies and generated output.

## Working in This Repository

- Read the relevant source before editing and follow existing patterns. Keep changes focused on the requested problem.
- Implement native behavior through existing Rust services and Tauri commands/events. Check callers and listeners when changing an interface.
- Check source and configuration before repeating technical examples or assumptions from documentation.

## Engineering Guardrails

- Preserve both audio outputs: recordings receive mixed audio, while transcription receives VAD-selected speech. A change to one path must not silently break the other.
- Preserve sample-rate conversions and the channel-drain/flush behavior during recording shutdown. Read the current pipeline and VAD implementation rather than assuming timing constants.
- Follow existing shared-state synchronization and async boundaries. Do not block async tasks or audio callbacks with expensive work.
- Use `perf_debug!()` and `perf_trace!()` for hot-path logging that should disappear in release builds. Keep pipeline and worker diagnostics aggregated rather than adding per-chunk logs.
- Reuse `AudioBufferPool` where applicable instead of adding avoidable audio-buffer allocations.
- Use Tauri path APIs for application files; do not hardcode platform-specific paths. Preserve platform permission handling.
- Use "microphone" and "system" consistently for audio-device roles. Keep errors user-friendly and logs diagnostic without exposing private meeting content.

## Source Map

| Area | Start here |
| --- | --- |
| Tauri entry point and command registration | [lib.rs](frontend/src-tauri/src/lib.rs) |
| Meeting persistence | [database/](frontend/src-tauri/src/database/) |
| Recording commands and lifecycle | [recording_commands.rs](frontend/src-tauri/src/audio/recording_commands.rs), [recording_manager.rs](frontend/src-tauri/src/audio/recording_manager.rs) |
| Device discovery and platform configuration | [devices/](frontend/src-tauri/src/audio/devices/) |
| Audio permissions | [permissions.rs](frontend/src-tauri/src/audio/permissions.rs) |
| Audio capture | [capture/](frontend/src-tauri/src/audio/capture/) |
| Mixing, resampling, and VAD | [pipeline.rs](frontend/src-tauri/src/audio/pipeline.rs), [vad.rs](frontend/src-tauri/src/audio/vad.rs) |
| Recording output | [recording_saver.rs](frontend/src-tauri/src/audio/recording_saver.rs) |
| Transcription providers and workers | [transcription/](frontend/src-tauri/src/audio/transcription/) |
| Whisper model loading and acceleration | [whisper_engine/](frontend/src-tauri/src/whisper_engine/) |
| Parakeet model loading and inference | [parakeet_engine/](frontend/src-tauri/src/parakeet_engine/) |
| audio.cpp engine FFI layer (native library loading, ABI gate) | [audio_cpp_engine/](frontend/src-tauri/src/audio_cpp_engine/) |
| audio.cpp model catalog and downloads | [catalog.rs](frontend/src-tauri/src/audio_cpp_engine/catalog.rs), [models.rs](frontend/src-tauri/src/audio_cpp_engine/models.rs), [AudioCppModelManager.tsx](frontend/src/components/AudioCppModelManager.tsx) |
| Summary providers and orchestration | [summary/](frontend/src-tauri/src/summary/) |
| Recording UI and shared frontend state | [page.tsx](frontend/src/app/page.tsx), [SidebarProvider.tsx](frontend/src/components/Sidebar/SidebarProvider.tsx), [RecordingStateContext.tsx](frontend/src/contexts/RecordingStateContext.tsx) |

## Verification and Privacy

- Exercise the changed path in the Tauri app when application behavior changes. Check relevant platform permissions, devices, and models for native/audio work.
- Use existing frontend tests under `frontend/tests/` and Rust tests for the affected subsystem. Inspect their runner and configuration before choosing commands; do not assume a `pnpm test` script exists.
- For documentation-only changes, check links and any changed commands or configuration rather than building the app unnecessarily.
- Report exactly what ran, observed results, the platform, and what remains untested. Never invent build, test, or hardware results.
- Use synthetic or redacted reproduction data. Never share private recordings, transcripts, credentials, or unredacted logs and attachments.

## Issues, PRs, and Authorization

- When preparing an issue or PR, read and follow [CONTRIBUTING.md](CONTRIBUTING.md) for duplicate searches, coordination with existing work, branch targets, review expectations, and submission requirements. Local-only edits and investigations do not require a GitHub duplicate search.
- Do not commit, push, create issues or PRs, or post comments without explicit user authorization for that action. Permission to edit locally is not permission to publish.
- Before requesting publication authorization, provide related issue/PR links, a concise change explanation, verification results, and known limitations.
