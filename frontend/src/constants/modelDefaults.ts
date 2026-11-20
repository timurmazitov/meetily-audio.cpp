/**
 * Default model names for transcription engines.
 * IMPORTANT: Keep in sync with Rust constants in src-tauri/src/config.rs
 */

/**
 * Default Transcribe.cpp model for transcription when no preference is
 * configured. Q8_0 is the recommended quality/size balance for this engine.
 * IMPORTANT: Keep in sync with DEFAULT_TRANSCRIBE_CPP_MODEL in
 * src-tauri/src/config.rs and the catalog in
 * src-tauri/src/transcribe_cpp_engine/catalog.rs.
 */
export const DEFAULT_TRANSCRIBE_CPP_MODEL = 'parakeet-ultra-Q8_0';

/**
 * Model defaults by provider type.
 *
 * transcribe.cpp is the only local transcription engine (the settings
 * repository soft-migrates the removed `localWhisper`/`parakeet`/`audioCpp`
 * providers to it), so it is the only default.
 */
export const MODEL_DEFAULTS = {
  transcribeCpp: DEFAULT_TRANSCRIBE_CPP_MODEL,
} as const;
