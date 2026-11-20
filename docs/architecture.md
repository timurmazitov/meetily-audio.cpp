# System Architecture

Samobranka is a self-contained desktop application built with [Tauri](https://tauri.app/). It combines a Rust-based backend with a Next.js frontend into a single, efficient, and cross-platform application.

## High-Level Architecture Diagram

```mermaid
graph TD
    subgraph User Interface
        A[Next.js Frontend]
    end

    subgraph "Core Logic (Rust)"
        B[Tauri Core]
        C[Audio Engine]
        D[Transcription Engine]
        E[Database]
        F[Summary Engine]
    end

    A -- Tauri Commands --> B
    B -- Manages --> C
    B -- Manages --> D
    B -- Manages --> E
    B -- Manages --> F
```

## Component Details

### Frontend (Next.js)

*   Provides the user interface for managing meetings, displaying transcriptions, and configuring the application.
*   Communicates with the Rust core through Tauri's command system.

### Backend (Rust Core)

*   **Tauri Core:** The heart of the application, responsible for managing the window, handling events, and exposing the Rust core to the frontend.
*   **Audio Engine:** Captures audio from the microphone and system, processes it, and prepares it for transcription.
*   **Transcription Engine:** Uses local speech-to-text models (Whisper, Parakeet, audioCpp via audio.cpp/GigaAM, or Transcribe.cpp) to transcribe the captured audio. It can be accelerated with a GPU. The audioCpp provider is a third engine, integrated in-process through a C ABI — see [ADR 0001](adr/0001-audio-cpp-as-third-engine.md). The transcribeCpp provider is a fourth engine on the same in-process FFI pattern (prebuilt transcribe.cpp runtime, Vulkan/CPU compute modes) — see [ADR 0006](adr/0006-transcribe-cpp-as-fourth-engine.md).
*   **Database:** A local SQLite database that stores meeting metadata, transcripts, and summaries.
*   **Summary Engine:** Generates meeting summaries using various Large Language Models (LLMs), including local models via Ollama.

## Frontend and Native Communication

The supported application communicates through Tauri commands and events; there is no separate service to run alongside the app.

- Frontend code calls Rust commands with `invoke` from `@tauri-apps/api/core`. Command registration lives in [lib.rs](../frontend/src-tauri/src/lib.rs); implementations delegate to the relevant Rust service.
- Rust emits events through Tauri, and frontend listeners update React state. Check the current command arguments and event payloads before changing either side of the interface.
- [SidebarProvider.tsx](../frontend/src/components/Sidebar/SidebarProvider.tsx) manages meeting navigation and summary polling, and uses the recording-state context. Preserve the existing state ownership rather than introducing a second source of truth.
- Speaker names have their own interface: the directory and per-meeting assignment tables, four commands, and the `speaker-names-reset` event are documented in [speaker-names.md](speaker-names.md).

## Audio Pipeline

[pipeline.rs](../frontend/src-tauri/src/audio/pipeline.rs) receives asynchronous microphone and system-audio chunks, converts input to mono, resamples when necessary, and buffers samples for mixing.

The mixed audio has two consumers with different requirements:

- **Recording:** mixed audio is sent to [recording_saver.rs](../frontend/src-tauri/src/audio/recording_saver.rs), which uses incremental saving and tracks recording metadata and transcript segments.
- **Transcription:** [vad.rs](../frontend/src-tauri/src/audio/vad.rs) selects speech segments and resamples them for the transcription workers. Silence filtering must not remove audio from the recording output.

The input-processing target is 48 kHz; VAD-selected transcription segments are sent at 16 kHz. Timing and buffering settings belong to the implementation, so inspect the current code rather than assuming a fixed mixing-window duration. During shutdown, the pipeline drains its channel and flushes remaining speech before completing.

On Linux, [pulse_linux.rs](../frontend/src-tauri/src/audio/capture/pulse_linux.rs) captures microphone and system audio through the PulseAudio protocol, including on PipeWire. It dispatches pending mainloop events in bounded nonblocking bursts, sleeping only when idle, so capture can keep up without starving sample delivery or stop checks.

For audio changes, start in the relevant subsystem:

- Device discovery and platform configuration: [devices/](../frontend/src-tauri/src/audio/devices/).
- Stream capture and platform integration: [capture/](../frontend/src-tauri/src/audio/capture/) and [stream.rs](../frontend/src-tauri/src/audio/stream.rs).
- Recording lifecycle: [recording_manager.rs](../frontend/src-tauri/src/audio/recording_manager.rs) and [recording_commands.rs](../frontend/src-tauri/src/audio/recording_commands.rs).
- Mixing and speech segmentation: [pipeline.rs](../frontend/src-tauri/src/audio/pipeline.rs) and [vad.rs](../frontend/src-tauri/src/audio/vad.rs).
- Provider dispatch and transcript emission: [transcription/](../frontend/src-tauri/src/audio/transcription/).

## Transcription and Model Management

Processing a meeting has two stages, and they are independent. **Transcription** is the first stage: it is implemented through the Whisper, Parakeet, or audioCpp providers under [audio/transcription/](../frontend/src-tauri/src/audio/transcription/). **Diarization** is the second stage: an offline pass over the finished transcript that overlays speaker turns onto the VAD segment timestamps, whatever provider produced the text. `diarize_saved_meeting` in [diarization_run.rs](../frontend/src-tauri/src/audio/transcription/diarization_run.rs) is the one operation behind every save path (recording stop, import, retranscription, manual run); it decides on its own whether a pass can run, serializes passes process-wide, writes the labels together with the speaker-name recount, and emits the native events. Its preconditions are an installed Nemotron-3 Diarization model and a loadable audio.cpp native library; there is no separate toggle, and a missing precondition degrades to a status (`no_model`, `no_runtime`) instead of failing the save. See [ADR 0004](adr/0004-diarization-independent-of-transcription-provider.md).

Whisper model discovery, downloads, loading, and cached context are managed by [whisper_engine.rs](../frontend/src-tauri/src/whisper_engine/whisper_engine.rs).

The audio.cpp provider keeps its model catalog and download lifecycle in [audio_cpp_engine/](../frontend/src-tauri/src/audio_cpp_engine/): `catalog.rs` describes each supported model (audio.cpp family, task, quantization, pinned upstream revision, exact size) and `models.rs` owns discovery, download, cancellation and deletion. Both are engine-independent — model management never loads the native library — so models can be managed before transcription is wired up, and a diarization model can be downloaded even when the native runtime is unavailable. Downloads use the shared [model_downloader.rs](../frontend/src-tauri/src/model_downloader.rs) mechanism and validate the GGUF header plus the exact catalogued size; model state is derived from disk on every query, so it survives restarts.

The settings UI reuses [AudioCppModelManager.tsx](../frontend/src/components/AudioCppModelManager.tsx) with a `task` parameter: the `audioCpp` provider's block renders the manager without a task and is shown only when that provider is selected (the whole catalog is listed there, but only transcription models are selectable), while the diarization list (`task="diar"`) lives in its own always-visible "Speaker diarization" section and is shown for every provider. Two managers can be mounted at once, so each one ignores native download events for the other task.

Model directories and acceleration behavior depend on application configuration and the compiled backend. Use the current initialization and loading code as the source of truth rather than hardcoding storage paths or assuming runtime GPU fallback. See the [build instructions](BUILDING.md) and [GPU acceleration guide](GPU_ACCELERATION.md) for platform setup.
