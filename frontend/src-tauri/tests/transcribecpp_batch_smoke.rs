//! Live batch smoke for the transcribe.cpp provider (ticket 07 manual
//! verification), `#[ignore]`d so CI never runs it.
//!
//! It mirrors the batch paths (`audio::retranscription` / `audio::import`)
//! minus the Tauri app, database and dialogs — the parts a headless run can
//! reach: decode → VAD with the batch redemption time → silence split at the
//! 25 s cap → skip <1600-sample segments → per-segment
//! `provider.transcribe(samples, language)` on the session thread →
//! post-batch unload releases the global provider and joins the session
//! thread.
//!
//! Requirements (the test skips with a printed reason when any is absent):
//! - the staged runtime at `src-tauri/transcribecpp-runtime/` (ticket 01
//!   staging script) or `TRANSCRIBECPP_LIBRARY_PATH` set;
//! - a model artifact: `SAMOBRANKA_TRANSCRIBECPP_MODEL` pointing at a .gguf,
//!   or one downloaded under the app-data models directory
//!   (`<app_data>/models/transcribecpp`);
//! - an audio file at `SAMOBRANKA_TRANSCRIBECPP_WAV` (any format the decoder
//!   accepts, speech included; e.g. generated with
//!   `ffmpeg -f lavfi -i "flite=text='hello world':voice=slt" -ar 16000 -ac 1 speech.wav`).
//!
//! Run with:
//! `SAMOBRANKA_TRANSCRIBECPP_WAV=speech.wav cargo test --test transcribecpp_batch_smoke -- --ignored --nocapture`.

use std::path::{Path, PathBuf};

use app_lib::audio::common::split_segment_at_silence;
use app_lib::audio::decoder::decode_audio_file;
use app_lib::audio::transcription::provider::TranscriptionProvider;
use app_lib::audio::vad::get_speech_chunks_with_progress;
use app_lib::transcribe_cpp_engine::backend::ComputeMode;
use app_lib::transcribe_cpp_engine::catalog::{ModelInfo, ModelStatus};
use app_lib::transcribe_cpp_engine::transcription::{
    load_native_engine, provider, set_provider, unload_provider,
};
use app_lib::transcribe_cpp_engine::wrapper::TranscribeCpp;

/// The staged library, mirroring `transcribecpp_live_smoke`.
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

/// Builds the `ModelInfo` the batch dispatcher would hand to
/// `load_native_provider`, with the real artifact path and Available status.
fn model_info(path: &Path) -> ModelInfo {
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("model file stem")
        .to_string();
    ModelInfo {
        name,
        display_name: "smoke".to_string(),
        family: "smoke".to_string(),
        quantization: "smoke".to_string(),
        filename: path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("file name")
            .to_string(),
        path: path.to_path_buf(),
        size_mb: 0,
        size_bytes: 0,
        description: String::new(),
        sha256: String::new(),
        status: ModelStatus::Available,
    }
}

/// The exact constants the batch paths use (`audio::retranscription`).
const VAD_REDEMPTION_TIME_MS: u32 = 2000;
const MAX_SEGMENT_SAMPLES: usize = 25 * 16000;
const MIN_SEGMENT_SAMPLES: usize = 1600;

#[tokio::test]
#[ignore = "live native batch smoke: requires the staged transcribe.cpp runtime, a downloaded model and SAMOBRANKA_TRANSCRIBECPP_WAV"]
async fn batch_retranscription_flow_transcribes_a_real_file_and_unloads() {
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

    // 1. Decode + resample exactly as the batch paths do.
    let decoded = decode_audio_file(&wav_path).expect("decode");
    let samples = decoded.to_whisper_format();
    eprintln!(
        "decoded {:.2}s @ {}Hz x{}ch -> {} samples @16k",
        decoded.duration_seconds,
        decoded.sample_rate,
        decoded.channels,
        samples.len()
    );

    // 2. VAD with the batch redemption time (blocking work off the runtime,
    //    as the real paths schedule it).
    let speech_segments = tokio::task::spawn_blocking(move || {
        get_speech_chunks_with_progress(&samples, VAD_REDEMPTION_TIME_MS, |_, _| true)
    })
    .await
    .expect("VAD task")
    .expect("VAD");
    eprintln!("vad: {} speech segments", speech_segments.len());
    assert!(!speech_segments.is_empty(), "the clip must contain speech");

    // 3. Silence split at the 25 s cap, same as retranscription.rs.
    let mut processable = Vec::new();
    for segment in &speech_segments {
        if segment.samples.len() > MAX_SEGMENT_SAMPLES {
            processable.extend(split_segment_at_silence(segment, MAX_SEGMENT_SAMPLES));
        } else {
            processable.push(segment.clone());
        }
    }

    // 4. Load the provider through the global handle (session thread) and
    //    transcribe each processable segment with the language passthrough,
    //    exactly like the BatchEngine::TranscribeCpp arms.
    let engine = load_native_engine(ComputeMode::Cpu).expect("CPU engine");
    let model = model_info(&model_path);
    let cpp_provider =
        app_lib::audio::transcription::transcribe_cpp_provider::TranscribeCppProvider::load(
            engine,
            model.name.clone(),
            model.path.clone(),
        )
        .await
        .expect("model loads");
    eprintln!(
        "model '{}' loaded on backend '{}'",
        cpp_provider.model_name(),
        cpp_provider.backend()
    );
    set_provider(cpp_provider);

    let language = Some("en".to_string());
    let mut texts = Vec::new();
    let transcribe_result = {
        let handle = provider().expect("provider is set");
        let mut result = Ok(());
        for (i, segment) in processable.iter().enumerate() {
            if segment.samples.len() < MIN_SEGMENT_SAMPLES {
                continue; // same skip rule as the batch paths
            }
            match handle
                .transcribe(segment.samples.clone(), language.clone())
                .await
            {
                Ok(transcript) => {
                    eprintln!(
                        "segment {i}: {:.1}s -> {:?}",
                        (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0,
                        transcript.text
                    );
                    texts.push(transcript.text);
                }
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        result
    };

    // 5. Post-batch unload: the global provider must be gone and the session
    //    thread joined (this returning proves the join did not hang).
    unload_provider();
    assert!(provider().is_none(), "the batch must drop the global provider");

    transcribe_result.expect("every segment transcribes");
    assert!(
        texts.iter().any(|text| !text.trim().is_empty()),
        "a speech clip must produce non-empty text"
    );
}
