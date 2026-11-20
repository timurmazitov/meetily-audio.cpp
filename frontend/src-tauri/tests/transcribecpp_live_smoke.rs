//! Live end-to-end smoke for the transcribe.cpp provider (ticket 05 manual
//! verification), `#[ignore]`d so CI never runs it.
//!
//! It exercises the real staged native library and a real model against a
//! real audio file, mirroring the production path minus the worker and the
//! Tauri app: resolve + ABI-gate + dlopen → `NativeTranscribeCppEngine` on
//! strict CPU → provider load on the session thread → offline ASR run.
//!
//! Requirements (the test skips with a printed reason when any is absent):
//! - the staged runtime at `src-tauri/transcribecpp-runtime/` (ticket 01
//!   staging script) or `TRANSCRIBECPP_LIBRARY_PATH` set;
//! - a model artifact: `SAMOBRANKA_TRANSCRIBECPP_MODEL` pointing at a .gguf,
//!   or one downloaded under the app-data models directory
//!   (`<app_data>/models/transcribecpp`, e.g.
//!   `~/.local/share/ai.samobranka.app/models/transcribecpp` on Linux);
//! - speech audio at `SAMOBRANKA_TRANSCRIBECPP_WAV` (16 kHz mono PCM wav;
//!   e.g. generated with
//!   `ffmpeg -f lavfi -i "flite=text='hello world':voice=slt" -ar 16000 -ac 1 speech.wav`).
//!
//! Run with: `cargo test --test transcribecpp_live_smoke -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use app_lib::audio::transcription::provider::TranscriptionProvider;
use app_lib::transcribe_cpp_engine::backend::ComputeMode;
use app_lib::transcribe_cpp_engine::engine::SAMPLE_RATE;
use app_lib::transcribe_cpp_engine::transcription::load_native_engine;
use app_lib::transcribe_cpp_engine::transcription::{provider, set_provider, unload_provider};
use app_lib::transcribe_cpp_engine::wrapper::TranscribeCpp;

fn staged_or_env_library() -> Option<PathBuf> {
    if let Some(env_path) = std::env::var_os("TRANSCRIBECPP_LIBRARY_PATH") {
        let path = PathBuf::from(env_path);
        return path.is_file().then_some(path);
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("transcribecpp-runtime")
        .join("libtranscribe.so");
    path.is_file().then_some(path)
}

fn candidate_models() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(model) = std::env::var_os("SAMOBRANKA_TRANSCRIBECPP_MODEL") {
        candidates.push(PathBuf::from(model));
    }
    // Tauri app_data_dir on Linux: $XDG_DATA_HOME or ~/.local/share + identifier.
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".local/share")
        });
    let dir = data_home
        .join("ai.samobranka.app")
        .join("models")
        .join("transcribecpp");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "gguf") {
                candidates.push(path);
            }
        }
    }
    candidates.retain(|path| path.is_file());
    candidates
}

/// Minimal RIFF/WAVE reader: first 16-bit PCM chunk, first channel, at its
/// declared rate; the test audio is produced at 16 kHz so no resample is
/// needed.
fn read_wav_mono_16k(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read wav");
    assert!(
        bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "not a wav"
    );

    let mut channels = 1u16;
    let mut samples: Vec<f32> = Vec::new();
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body = offset + 8;
        match id {
            b"fmt " => {
                channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
                let sample_rate =
                    u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
                let bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
                assert_eq!(sample_rate, 16_000, "test audio must be 16 kHz");
                assert_eq!(bits, 16, "test audio must be PCM16");
            }
            b"data" => {
                let end = (body + size).min(bytes.len());
                let stride = channels as usize * 2;
                samples = bytes[body..end]
                    .chunks_exact(stride)
                    .map(|frame| {
                        i16::from_le_bytes(frame[0..2].try_into().unwrap()) as f32 / 32_768.0
                    })
                    .collect();
                break;
            }
            _ => {}
        }
        offset = body + size + (size % 2);
    }
    assert!(!samples.is_empty(), "no PCM samples found");
    samples
}

#[tokio::test]
#[ignore = "live native smoke: requires the staged transcribe.cpp runtime, a downloaded model and SAMOBRANKA_TRANSCRIBECPP_WAV"]
async fn staged_library_and_model_transcribe_speech_end_to_end() {
    let Some(library_path) = staged_or_env_library() else {
        eprintln!("SKIP: staged transcribe.cpp runtime not present");
        return;
    };
    let Some(model_path) = candidate_models().into_iter().next() else {
        eprintln!("SKIP: no transcribe.cpp model downloaded");
        return;
    };
    let Some(wav_path) = std::env::var_os("SAMOBRANKA_TRANSCRIBECPP_WAV").map(PathBuf::from) else {
        eprintln!("SKIP: SAMOBRANKA_TRANSCRIBECPP_WAV not set");
        return;
    };

    // The gate + dlopen must pass for the staged lane, on strict CPU so the
    // smoke does not depend on a GPU driver.
    let library = TranscribeCpp::load_from(&library_path).expect("staged library passes the gate");
    eprintln!(
        "library {} at {} (backends {:?})",
        library.version(),
        library.path().display(),
        library.contract().backends
    );

    let samples = read_wav_mono_16k(&wav_path);
    eprintln!("audio: {} samples @ {SAMPLE_RATE} Hz", samples.len());

    // Production path: engine over the library, provider through the global
    // handle so the session thread and unload are exercised too.
    let engine = load_native_engine(ComputeMode::Cpu).expect("CPU engine");
    let name = model_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("smoke-model")
        .to_string();
    let cpp_provider = app_lib::audio::transcription::transcribe_cpp_provider::TranscribeCppProvider::load(
        engine,
        name.clone(),
        model_path.clone(),
    )
    .await
    .expect("model loads");
    eprintln!("model '{name}' loaded on backend '{}'", cpp_provider.backend());
    set_provider(cpp_provider, ComputeMode::Cpu);

    let result = {
        let handle = provider().expect("provider is set");
        handle.transcribe(samples, Some("en".to_string())).await
    };
    unload_provider();

    let result = result.expect("transcription succeeds");
    eprintln!("transcript: {:?}", result.text);
    assert!(
        !result.text.trim().is_empty(),
        "a speech clip must produce non-empty text"
    );
    assert!(!result.is_partial);
}

/// Manual Vulkan verification (ticket 06 checkbox): loads the engine the way
/// the saved «Vulkan» mode does and records what actually happened — on a GPU
/// machine the model must bind to vulkan; without a driver the load must
/// still succeed with the CPU fallback flagged for the warning channel.
/// Never fails on missing hardware: it reports the fact and passes.
///
/// Run it ALONE (`-- --ignored --exact
/// forced_vulkan_reports_the_actual_backend`): the allowed-backend mask is
/// fixed process-wide, so running it together with the CPU smoke in the same
/// binary would honestly report a mask-already-fixed CPU fallback rather than
/// the GPU fact.
#[tokio::test]
#[ignore = "live native smoke: requires the staged transcribe.cpp runtime; run on a GPU machine to record the Vulkan fact"]
async fn forced_vulkan_reports_the_actual_backend() {
    let Some(library_path) = staged_or_env_library() else {
        eprintln!("SKIP: staged transcribe.cpp runtime not present");
        return;
    };

    // The engine constructor is the production Vulkan path (mask gate +
    // device selection); provider and session threads are covered by the
    // CPU smoke above.
    let library = TranscribeCpp::load_from(&library_path).expect("staged library passes the gate");
    let engine = load_native_engine(ComputeMode::Vulkan).expect("vulkan request must never fail");
    let selection = engine.selection().clone();
    let contract_backends = library.contract().backends.clone();
    drop(engine);

    if selection.fell_back_to_cpu {
        eprintln!(
            "Vulkan requested, contract backends {contract_backends:?}: fell back to CPU \
             (the dispatcher warns the user) — acceptable without a GPU driver"
        );
    } else {
        eprintln!(
            "Vulkan requested, contract backends {contract_backends:?}: registered and \
             pinned device index {:?} — GPU acceleration in effect",
            selection.device_index
        );
        assert_eq!(selection.mode, ComputeMode::Vulkan);
    }
}
