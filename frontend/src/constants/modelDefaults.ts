/**
 * Default model names for transcription engines.
 * IMPORTANT: Keep in sync with Rust constants in src-tauri/src/config.rs
 */

/**
 * Default Whisper model for transcription when no preference is configured.
 * This is the recommended balance of accuracy and speed.
 */
export const DEFAULT_WHISPER_MODEL = 'large-v3-turbo';

/**
 * Default Parakeet model for transcription when no preference is configured.
 * This is the quantized version optimized for speed.
 */
export const DEFAULT_PARAKEET_MODEL = 'parakeet-tdt-0.6b-v3-int8';

/**
 * Default Transcribe.cpp model for transcription when no preference is
 * configured. Q8_0 is the recommended quality/size balance for this engine.
 * IMPORTANT: Keep in sync with DEFAULT_TRANSCRIBE_CPP_MODEL in
 * src-tauri/src/config.rs and the catalog in
 * src-tauri/src/transcribe_cpp_engine/catalog.rs.
 */
export const DEFAULT_TRANSCRIBE_CPP_MODEL = 'parakeet-ultra-Q8_0';

/**
 * Model defaults by provider type
 */
export const MODEL_DEFAULTS = {
  whisper: DEFAULT_WHISPER_MODEL,
  localWhisper: DEFAULT_WHISPER_MODEL,
  parakeet: DEFAULT_PARAKEET_MODEL,
  transcribeCpp: DEFAULT_TRANSCRIBE_CPP_MODEL,
} as const;
