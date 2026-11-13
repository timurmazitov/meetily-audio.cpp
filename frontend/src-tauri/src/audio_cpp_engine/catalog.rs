//! Static catalog of the models the audio.cpp provider can use.
//!
//! The catalog is deliberately free of any engine interaction: it describes
//! what a model *is* — its audio.cpp family (`gigaam_asr`,
//! `nemotron_3_diar`), its task (`asr` / `diar`), the quantization of the
//! published weights, the exact artifact size and where the weights live
//! upstream. Nothing here loads the native library, so a model can be listed,
//! downloaded, validated and deleted before transcription support exists.
//!
//! Sources are pinned to a HuggingFace commit so that the artifact a user
//! downloads is the artifact the catalog describes:
//! - GigaAM v3 e2e CTC F16: `audio-cpp/audio.cpp-gguf`
//!   (`GigaAM-ASR-GGUF/gigaam-v3-e2e-ctc-f16.gguf`). The handy-computer
//!   package is built for transcribe.cpp and is not used here.
//! - Nemotron-3-Diarization BF16: `audio-cpp/Nemotron-3-Diarization-GGUF`.
//!   Q8_0 is not catalogued because it is not parity-safe for speaker turns.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// The task a model performs, in audio.cpp terms (`asr` / `diar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTask {
    /// Speech recognition (`audiocpp_model_supports(model, "asr", ...)`).
    Asr,
    /// Speaker diarization (`audiocpp_model_supports(model, "diar", ...)`).
    Diar,
}

impl ModelTask {
    /// The task string audio.cpp uses in its model specs and capability
    /// queries.
    pub fn audio_cpp_name(self) -> &'static str {
        match self {
            ModelTask::Asr => "asr",
            ModelTask::Diar => "diar",
        }
    }

    /// Human-facing task description for the model manager UI.
    pub fn label(self) -> &'static str {
        match self {
            ModelTask::Asr => "Transcription",
            ModelTask::Diar => "Diarization",
        }
    }
}

/// Installation state of a catalogued model. Mirrors the shape the Whisper
/// and Parakeet managers expose so the frontend model managers stay
/// consistent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading {
        progress: u8,
    },
    Error(String),
    Corrupted {
        file_size: u64,
        expected_min_size: u64,
    },
}

/// A catalogued model plus its on-disk state.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Stable model id used by the commands and the UI.
    pub name: String,
    /// Human-readable model name.
    pub display_name: String,
    /// audio.cpp model family, e.g. `gigaam_asr`.
    pub family: String,
    /// Task the model performs, independent of any engine.
    pub task: ModelTask,
    /// Weight quantization as published upstream, e.g. `F16` or `BF16`.
    pub quantization: String,
    /// File name inside the audio.cpp models directory.
    pub filename: String,
    /// Full path of the local artifact.
    pub path: PathBuf,
    /// Size in MiB, for display.
    pub size_mb: u32,
    /// Exact artifact size in bytes; downloads are validated against it.
    pub size_bytes: u64,
    /// Catalog description (purpose and trade-offs).
    pub description: String,
    /// Current installation state.
    pub status: ModelStatus,
}

/// Static description of one supported model.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub name: &'static str,
    pub display_name: &'static str,
    pub family: &'static str,
    pub task: ModelTask,
    pub quantization: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub exact_bytes: u64,
    pub description: &'static str,
    pub source_url: &'static str,
}

impl ModelSpec {
    /// Where this model's artifact lives inside `models_dir`.
    pub fn file_path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.filename)
    }

    /// Build the serializable description of this model for a given
    /// on-disk status.
    pub fn model_info(&self, models_dir: &Path, status: ModelStatus) -> ModelInfo {
        ModelInfo {
            name: self.name.to_string(),
            display_name: self.display_name.to_string(),
            family: self.family.to_string(),
            task: self.task,
            quantization: self.quantization.to_string(),
            filename: self.filename.to_string(),
            path: self.file_path(models_dir),
            size_mb: self.size_mb,
            size_bytes: self.exact_bytes,
            description: self.description.to_string(),
            status,
        }
    }
}

/// Models supported by the audio.cpp provider, in display order.
pub const MODEL_CATALOG: &[ModelSpec] = &[
    ModelSpec {
        name: "gigaam-v3-e2e-ctc-f16",
        display_name: "GigaAM v3 e2e CTC",
        family: "gigaam_asr",
        task: ModelTask::Asr,
        quantization: "F16",
        filename: "gigaam-v3-e2e-ctc-f16.gguf",
        // 442_520_704 bytes = 422 MiB.
        size_mb: 422,
        exact_bytes: 442_520_704,
        description:
            "Russian speech recognition (GigaAM v3 end-to-end CTC, F16). Best accuracy per size for Russian meetings.",
        source_url: concat!(
            "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/73ce4162f6cce370dd533c6782b772135c1b2a8e",
            "/GigaAM-ASR-GGUF/gigaam-v3-e2e-ctc-f16.gguf"
        ),
    },
    ModelSpec {
        name: "nemotron-3-diarization-bf16",
        display_name: "Nemotron-3 Diarization",
        family: "nemotron_3_diar",
        task: ModelTask::Diar,
        quantization: "BF16",
        filename: "nemotron-3-diarization-bf16.gguf",
        // 198_720_928 bytes = 190 MiB.
        size_mb: 190,
        exact_bytes: 198_720_928,
        description:
            "Speaker diarization (Nemotron-3, BF16, up to 8 speakers by arrival order). Attributes transcript segments to speakers.",
        source_url: concat!(
            "https://huggingface.co/audio-cpp/Nemotron-3-Diarization-GGUF/resolve/647d39feaa0e91dca5ce355a95403837b76dff56",
            "/nemotron-3-diarization-bf16.gguf"
        ),
    },
];

/// The full catalog.
pub fn catalog() -> &'static [ModelSpec] {
    MODEL_CATALOG
}

/// Look up a catalogued model by its stable name.
pub fn find_model(name: &str) -> Option<&'static ModelSpec> {
    MODEL_CATALOG.iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> &'static ModelSpec {
        find_model(name).unwrap_or_else(|| panic!("{name} must be catalogued"))
    }

    #[test]
    fn catalog_lists_gigaam_for_transcription() {
        let gigaam = spec("gigaam-v3-e2e-ctc-f16");

        assert_eq!(gigaam.family, "gigaam_asr");
        assert_eq!(gigaam.task, ModelTask::Asr);
        assert_eq!(gigaam.task.audio_cpp_name(), "asr");
        assert_eq!(gigaam.quantization, "F16");
    }

    #[test]
    fn catalog_lists_nemotron_diarization_bf16() {
        let nemotron = spec("nemotron-3-diarization-bf16");

        assert_eq!(nemotron.family, "nemotron_3_diar");
        assert_eq!(nemotron.task, ModelTask::Diar);
        assert_eq!(nemotron.task.audio_cpp_name(), "diar");
        assert_eq!(nemotron.quantization, "BF16");
    }

    #[test]
    fn gigaam_weights_come_from_audio_cpp_not_transcribe_cpp_packages() {
        let gigaam = spec("gigaam-v3-e2e-ctc-f16");

        assert!(
            gigaam
                .source_url
                .starts_with("https://huggingface.co/audio-cpp/audio.cpp-gguf/"),
            "unexpected source: {}",
            gigaam.source_url
        );
        assert!(gigaam.source_url.contains("GigaAM-ASR-GGUF/"));
        assert!(gigaam.source_url.ends_with("/gigaam-v3-e2e-ctc-f16.gguf"));
        assert!(!gigaam.source_url.contains("handy-computer"));
        assert!(!gigaam.source_url.contains("transcribe.cpp"));
    }

    #[test]
    fn nemotron_weights_come_from_the_audio_cpp_diarization_repository() {
        let nemotron = spec("nemotron-3-diarization-bf16");

        assert!(nemotron
            .source_url
            .starts_with("https://huggingface.co/audio-cpp/Nemotron-3-Diarization-GGUF/"));
        assert!(nemotron
            .source_url
            .ends_with("/nemotron-3-diarization-bf16.gguf"));
        assert!(!nemotron.source_url.contains("q8_0"));
    }

    #[test]
    fn model_id_matches_filename_stem() {
        for spec in catalog() {
            assert_eq!(
                spec.filename,
                format!("{}.gguf", spec.name),
                "model {} must be stored under its stable name",
                spec.name
            );
            let size_mib = (spec.exact_bytes as f64 / (1024.0 * 1024.0)).round() as u32;
            assert_eq!(spec.size_mb, size_mib, "model {} size_mb", spec.name);
        }
    }

    #[test]
    fn catalog_names_are_unique_and_models_are_found_by_name() {
        let mut names: Vec<&str> = catalog().iter().map(|spec| spec.name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "catalog names must be unique");

        for spec in catalog() {
            assert!(find_model(spec.name).is_some());
        }
        assert!(find_model("whisper-large").is_none());
    }

    #[test]
    fn model_info_carries_family_and_task_metadata() {
        let spec = spec("nemotron-3-diarization-bf16");
        let info = spec.model_info(Path::new("/models/audiocpp"), ModelStatus::Missing);

        assert_eq!(info.name, spec.name);
        assert_eq!(info.family, "nemotron_3_diar");
        assert_eq!(info.task, ModelTask::Diar);
        assert_eq!(info.quantization, "BF16");
        assert_eq!(info.filename, "nemotron-3-diarization-bf16.gguf");
        assert_eq!(
            info.path,
            PathBuf::from("/models/audiocpp/nemotron-3-diarization-bf16.gguf")
        );
        assert!(matches!(info.status, ModelStatus::Missing));
    }

    #[test]
    fn task_and_status_serialize_in_the_shape_the_ui_expects() {
        let info = spec("gigaam-v3-e2e-ctc-f16").model_info(
            Path::new("/models"),
            ModelStatus::Downloading { progress: 7 },
        );
        let json = serde_json::to_value(&info).unwrap();

        assert_eq!(json["task"], "asr");
        assert_eq!(json["status"]["Downloading"]["progress"], 7);

        let corrupted = serde_json::to_value(ModelStatus::Corrupted {
            file_size: 10,
            expected_min_size: 20,
        })
        .unwrap();
        assert_eq!(corrupted["Corrupted"]["file_size"], 10);
        assert_eq!(corrupted["Corrupted"]["expected_min_size"], 20);
    }
}
