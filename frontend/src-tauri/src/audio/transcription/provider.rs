// audio/transcription/provider.rs
//
// Defines the unified TranscriptionProvider trait and common types for all
// transcription engines (Whisper, Parakeet, future providers).

use async_trait::async_trait;

// ============================================================================
// TRANSCRIPTION PROVIDER TRAIT & ERROR TYPES
// ============================================================================

/// Granular error types for transcription operations
#[derive(Debug, Clone)]
pub enum TranscriptionError {
    ModelNotLoaded,
    AudioTooShort {
        samples: usize,
        minimum: usize,
    },
    EngineFailed(String),
    UnsupportedLanguage(String),
    /// The provider cannot translate audio; only Whisper can. The message
    /// names the limitation so the UI can show it verbatim.
    TranslationNotSupported,
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotLoaded => write!(f, "No transcription model is loaded"),
            Self::AudioTooShort { samples, minimum } => write!(
                f,
                "Audio too short: {} samples (minimum {})",
                samples, minimum
            ),
            Self::EngineFailed(msg) => write!(f, "Transcription engine failed: {}", msg),
            Self::UnsupportedLanguage(lang) => {
                write!(f, "Language '{}' is not supported by this provider", lang)
            }
            Self::TranslationNotSupported => write!(f, "{TRANSLATION_WHISPER_ONLY_MESSAGE}"),
        }
    }
}

impl std::error::Error for TranscriptionError {}

/// Language value that asks a provider to translate to English.
pub const AUTO_TRANSLATE_LANGUAGE: &str = "auto-translate";

/// User-facing explanation for the missing translation option, shown by the
/// settings UI and available to backend error paths.
pub const TRANSLATION_WHISPER_ONLY_MESSAGE: &str =
    "Automatic translation is only available with the Whisper provider. Select another language, or switch the transcript provider to Whisper.";

/// Whether a transcription provider can translate audio to English.
///
/// Only Whisper's decoder has a translate flag; audioCpp (GigaAM) and Parakeet
/// transcribe in the language they hear. This is the single source of truth
/// for the capability on the Rust side.
pub fn provider_supports_translation(provider_name: &str) -> bool {
    provider_name == "Whisper"
}

/// Refuses an automatic-translation hint for a provider that cannot translate.
///
/// The frontend disables the option; this is the backend half of the same
/// contract, so a stale saved preference cannot silently produce untranslated
/// text that the user expects to be English.
pub fn check_translation_support(
    provider_name: &str,
    language: Option<&str>,
) -> Result<(), TranscriptionError> {
    match language {
        Some(AUTO_TRANSLATE_LANGUAGE) if !provider_supports_translation(provider_name) => {
            Err(TranscriptionError::TranslationNotSupported)
        }
        _ => Ok(()),
    }
}

/// The language hint to hand a provider.
///
/// Translation is a per-provider capability, so a stale "auto-translate"
/// preference saved while Whisper was selected must not make another provider
/// claim translated output: it degrades to auto-detect instead.
pub fn language_hint_for_provider(
    provider_name: &str,
    preference: Option<String>,
) -> Option<String> {
    match preference.as_deref() {
        Some(AUTO_TRANSLATE_LANGUAGE) if !provider_supports_translation(provider_name) => {
            Some("auto".to_string())
        }
        _ => preference,
    }
}

/// Unified transcription result across all providers
#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub confidence: Option<f32>, // None if provider doesn't support confidence scores
    pub is_partial: bool,
}

/// Trait for transcription providers (Whisper, Parakeet, future providers)
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Transcribe audio samples to text
    ///
    /// # Arguments
    /// * `audio` - Audio samples (16kHz mono, f32 format)
    /// * `language` - Optional language hint (e.g., "en", "es", "fr")
    ///
    /// # Returns
    /// * `TranscriptResult` with text, optional confidence, and partial flag
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError>;

    /// Check if a model is currently loaded
    async fn is_model_loaded(&self) -> bool;

    /// Get the name of the currently loaded model
    async fn get_current_model(&self) -> Option<String>;

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whisper_can_translate() {
        assert!(provider_supports_translation("Whisper"));
        assert!(!provider_supports_translation("audioCpp"));
        assert!(!provider_supports_translation("Parakeet"));
    }

    #[test]
    fn translation_error_names_the_whisper_limitation() {
        let text = TranscriptionError::TranslationNotSupported.to_string();
        assert!(text.contains("Whisper"), "text: {text}");
        assert!(text.contains("translation"), "text: {text}");
    }

    #[test]
    fn a_translate_preference_is_degraded_for_providers_without_translation() {
        let translate = Some(AUTO_TRANSLATE_LANGUAGE.to_string());

        assert_eq!(
            language_hint_for_provider("Whisper", translate.clone()),
            translate
        );
        assert_eq!(
            language_hint_for_provider("audioCpp", translate.clone()),
            Some("auto".to_string())
        );
        assert_eq!(
            language_hint_for_provider("Parakeet", translate),
            Some("auto".to_string())
        );
    }

    #[test]
    fn a_stale_translate_preference_is_refused_for_providers_without_translation() {
        // The backend half of the frontend capability rule.
        assert!(check_translation_support("Whisper", Some(AUTO_TRANSLATE_LANGUAGE)).is_ok());
        for provider in ["audioCpp", "Parakeet"] {
            let result = check_translation_support(provider, Some(AUTO_TRANSLATE_LANGUAGE));
            assert!(
                matches!(result, Err(TranscriptionError::TranslationNotSupported)),
                "provider: {provider}"
            );
        }
        assert!(check_translation_support("audioCpp", Some("ru")).is_ok());
        assert!(check_translation_support("audioCpp", None).is_ok());
    }

    #[test]
    fn ordinary_language_hints_pass_through_unchanged() {
        for hint in [None, Some("ru".to_string()), Some("auto".to_string())] {
            assert_eq!(language_hint_for_provider("audioCpp", hint.clone()), hint);
            assert_eq!(language_hint_for_provider("Whisper", hint.clone()), hint);
        }
    }
}
