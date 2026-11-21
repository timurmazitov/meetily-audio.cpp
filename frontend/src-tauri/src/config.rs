/// Application configuration constants
///
/// Centralized definitions for default models and settings.
/// Used across database initialization, import, and retranscription.

/// Default transcribe.cpp model for transcription when no preference is
/// configured. Q8_0 is the recommended quality/size balance for this engine.
/// IMPORTANT: Keep in sync with `DEFAULT_TRANSCRIBE_CPP_MODEL` in
/// `frontend/src/constants/modelDefaults.ts`.
pub const DEFAULT_TRANSCRIBE_CPP_MODEL: &str = "parakeet-ultra-Q8_0";
