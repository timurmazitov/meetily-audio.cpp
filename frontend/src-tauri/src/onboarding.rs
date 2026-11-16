use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use log::{info, warn, error};
use anyhow::Result;

use crate::state::AppState;
use crate::database::repositories::setting::SettingsRepository;
use crate::audio::transcription::engine::AUDIO_CPP_PROVIDER;

/// audio.cpp transcription model onboarding installs and configures.
///
/// The catalog (`audio_cpp_engine::catalog`) is the source of truth for model
/// names; `onboarding_transcript_model_is_a_catalogued_asr_model` guards this
/// constant against drifting from it.
pub const ONBOARDING_TRANSCRIPT_MODEL: &str = "gigaam-v3-e2e-ctc-f16";


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnboardingStatus {
    pub version: String,
    pub completed: bool,
    pub current_step: u8,
    pub model_status: ModelStatus,
    pub last_updated: String,
}

/// Per-model onboarding progress.
///
/// `#[serde(default)]` keeps status files written by older builds readable:
/// unknown keys (e.g. the removed `parakeet`) are ignored by serde, and keys
/// this schema adds fall back to their `Default` value ("not_downloaded").
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ModelStatus {
    pub gigaam: String,      // "downloaded" | "not_downloaded" | "downloading"
    pub diarization: String, // "downloaded" | "not_downloaded" | "downloading"
    pub summary: String,     // Generic field for summary model (Qwen 3.5 or legacy Gemma variants)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_summary_model: Option<String>,
}

impl Default for ModelStatus {
    fn default() -> Self {
        Self {
            gigaam: "not_downloaded".to_string(),
            diarization: "not_downloaded".to_string(),
            summary: "not_downloaded".to_string(),
            selected_summary_model: None,
        }
    }
}

impl Default for OnboardingStatus {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            completed: false,
            current_step: 1,
            model_status: ModelStatus::default(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Transcript configuration onboarding writes on completion: the audio.cpp
/// provider with the GigaAM v3 e2e CTC model. Diarization is not part of the
/// config — it is resolved from the installed catalog models.
pub fn onboarding_transcript_config() -> (&'static str, &'static str) {
    (AUDIO_CPP_PROVIDER, ONBOARDING_TRANSCRIPT_MODEL)
}


/// Load onboarding status from store
pub async fn load_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<OnboardingStatus> {
    // Try to load from Tauri store
    let store = match app.store("onboarding-status.json") {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access onboarding store: {}, using defaults", e);
            return Ok(OnboardingStatus::default());
        }
    };

    // Try to get the status from store
    let status = if let Some(value) = store.get("status") {
        match serde_json::from_value::<OnboardingStatus>(value.clone()) {
            Ok(s) => {
                info!("Loaded onboarding status from store - Step: {}, Completed: {}",
                      s.current_step, s.completed);
                s
            }
            Err(e) => {
                warn!("Failed to deserialize onboarding status: {}, using defaults", e);
                OnboardingStatus::default()
            }
        }
    } else {
        info!("No stored onboarding status found, using defaults");
        OnboardingStatus::default()
    };

    Ok(status)
}

/// Save onboarding status to store
pub async fn save_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
    status: &OnboardingStatus,
) -> Result<()> {
    info!("Saving onboarding status: step={}, completed={}",
          status.current_step, status.completed);

    // Get or create store
    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Update last_updated timestamp
    let mut status = status.clone();
    status.last_updated = chrono::Utc::now().to_rfc3339();

    // Serialize status to JSON value
    let status_value = serde_json::to_value(&status)
        .map_err(|e| anyhow::anyhow!("Failed to serialize onboarding status: {}", e))?;

    // Save to store
    store.set("status", status_value);

    // Persist to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store to disk: {}", e))?;

    info!("Successfully persisted onboarding status to disk");
    Ok(())
}

/// Reset onboarding status (delete from store)
pub async fn reset_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<()> {
    info!("Resetting onboarding status");

    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Clear the status key
    store.delete("status");

    // Persist deletion to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store after reset: {}", e))?;

    info!("Successfully reset onboarding status");
    Ok(())
}

/// Tauri commands for onboarding status
#[tauri::command]
pub async fn get_onboarding_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<OnboardingStatus>, String> {
    let status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    // Return None if it's the default (never saved before)
    // Check if we have any saved data by seeing if the store has the key
    let store = app.store("onboarding-status.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    if store.get("status").is_none() {
        Ok(None)
    } else {
        Ok(Some(status))
    }
}

#[tauri::command]
pub async fn save_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
    status: OnboardingStatus,
) -> Result<(), String> {
    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save onboarding status: {}", e))
}

#[tauri::command]
pub async fn reset_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    reset_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to reset onboarding status: {}", e))
}

#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    info!("Completing onboarding with builtin-ai model: {}", model);

    // Step 1: Save model configuration to SQLite database FIRST
    let pool = state.db_manager.pool();

    // Onboarding always uses builtin-ai (local LLM)
    if let Err(e) = SettingsRepository::save_model_config(
        pool,
        "builtin-ai",
        &model,
        "large-v3",
        None,
    ).await {
        error!("Failed to save builtin-ai model config: {}", e);
        return Err(format!("Failed to save builtin-ai model config: {}", e));
    }
    info!("Saved builtin-ai model config: model={}", model);

    // Save transcription model config (audio.cpp provider) - GigaAM v3 e2e CTC
    let (transcript_provider, transcript_model) = onboarding_transcript_config();
    if let Err(e) = SettingsRepository::save_transcript_config(
        pool,
        transcript_provider,
        transcript_model,
    ).await {
        error!("Failed to save transcription model config: {}", e);
        return Err(format!("Failed to save transcription model config: {}", e));
    }
    info!(
        "Saved transcription model config: provider={}, model={}",
        transcript_provider, transcript_model
    );

    // Step 2: Only NOW mark onboarding as complete (after DB operations succeed)
    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.completed = true;
    status.current_step = 4; // Max step (4 on macOS with permissions, 3 on other platforms)
    status.model_status.gigaam = "downloaded".to_string();
    status.model_status.diarization = "downloaded".to_string();
    status.model_status.summary = "downloaded".to_string();
    status.model_status.selected_summary_model = Some(model.clone());

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;

    info!("Onboarding completed successfully with model: {}", model);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_status_deserializes_without_selected_summary_model() {
        let status: OnboardingStatus = serde_json::from_str(
            r#"{
                "version": "1.0",
                "completed": true,
                "current_step": 4,
                "model_status": {
                    "parakeet": "downloaded",
                    "summary": "downloaded"
                },
                "last_updated": "2026-05-30T00:00:00Z"
            }"#,
        )
        .expect("old onboarding status should remain compatible");

        assert_eq!(status.model_status.selected_summary_model, None);
        assert_eq!(status.model_status.summary, "downloaded");
    }

    #[test]
    fn legacy_parakeet_status_deserializes_into_audiocpp_schema() {
        // Written by an earlier build: only `parakeet` + `summary`. The
        // unknown `parakeet` field must be ignored and the new audio.cpp
        // fields must fall back to "not_downloaded".
        let status: OnboardingStatus = serde_json::from_str(
            r#"{
                "version": "1.0",
                "completed": false,
                "current_step": 3,
                "model_status": {
                    "parakeet": "downloaded",
                    "summary": "not_downloaded"
                },
                "last_updated": "2026-05-30T00:00:00Z"
            }"#,
        )
        .expect("legacy onboarding status must not fail to deserialize");

        assert_eq!(status.current_step, 3);
        assert!(!status.completed);
        assert_eq!(status.model_status.summary, "not_downloaded");
        assert_eq!(status.model_status.gigaam, "not_downloaded");
        assert_eq!(status.model_status.diarization, "not_downloaded");
    }

    #[test]
    fn onboarding_status_round_trips_with_audiocpp_model_fields() {
        let status = OnboardingStatus {
            model_status: ModelStatus {
                gigaam: "downloaded".to_string(),
                diarization: "downloading".to_string(),
                summary: "downloaded".to_string(),
                selected_summary_model: Some("qwen3.5:4b".to_string()),
            },
            ..OnboardingStatus::default()
        };

        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains(r#""gigaam":"downloaded""#), "{json}");
        assert!(json.contains(r#""diarization":"downloading""#), "{json}");
        assert!(!json.contains("parakeet"), "parakeet must be gone: {json}");

        let parsed: OnboardingStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.model_status.gigaam, "downloaded");
        assert_eq!(parsed.model_status.diarization, "downloading");
        assert_eq!(parsed.model_status.summary, "downloaded");
        assert_eq!(
            parsed.model_status.selected_summary_model.as_deref(),
            Some("qwen3.5:4b")
        );
    }

    #[test]
    fn default_model_status_is_not_downloaded() {
        let status = ModelStatus::default();

        assert_eq!(status.gigaam, "not_downloaded");
        assert_eq!(status.diarization, "not_downloaded");
        assert_eq!(status.summary, "not_downloaded");
        assert_eq!(status.selected_summary_model, None);
    }

    #[test]
    fn onboarding_selects_audiocpp_gigaam_transcript_config() {
        let (provider, model) = onboarding_transcript_config();

        assert_eq!(provider, "audioCpp");
        assert_eq!(model, "gigaam-v3-e2e-ctc-f16");
        assert_eq!(provider, crate::audio::transcription::engine::AUDIO_CPP_PROVIDER);
    }

    #[test]
    fn onboarding_transcript_model_is_a_catalogued_asr_model() {
        let (_, model) = onboarding_transcript_config();
        let spec = crate::audio_cpp_engine::catalog::find_model(model)
            .expect("onboarding transcript model must be catalogued");

        assert_eq!(spec.task, crate::audio_cpp_engine::catalog::ModelTask::Asr);
    }
}
