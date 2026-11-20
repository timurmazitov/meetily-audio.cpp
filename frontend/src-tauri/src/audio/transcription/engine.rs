// audio/transcription/engine.rs
//
// TranscriptionEngine enum and model initialization/validation logic.

use super::provider::TranscriptionProvider;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

// ============================================================================
// TRANSCRIPTION ENGINE ENUM
// ============================================================================

// Transcription engine abstraction to support multiple providers
pub enum TranscriptionEngine {
    Whisper(Arc<crate::whisper_engine::WhisperEngine>), // Direct access (backward compat)
    Parakeet(Arc<crate::parakeet_engine::ParakeetEngine>), // Direct access (backward compat)
    Provider(Arc<dyn TranscriptionProvider>),           // Trait-based (preferred for new code)
}

/// Provider id for the audio.cpp engine (GLOSSARY: «провайдер транскрибации»).
pub const AUDIO_CPP_PROVIDER: &str = "audioCpp";

/// Provider id for the transcribe.cpp engine (GLOSSARY: «провайдер транскрибации»).
pub const TRANSCRIBE_CPP_PROVIDER: &str = "transcribeCpp";

impl TranscriptionEngine {
    /// Check if the engine has a model loaded
    pub async fn is_model_loaded(&self) -> bool {
        match self {
            Self::Whisper(engine) => engine.is_model_loaded().await,
            Self::Parakeet(engine) => engine.is_model_loaded().await,
            Self::Provider(provider) => provider.is_model_loaded().await,
        }
    }

    /// Get the current model name
    pub async fn get_current_model(&self) -> Option<String> {
        match self {
            Self::Whisper(engine) => engine.get_current_model().await,
            Self::Parakeet(engine) => engine.get_current_model().await,
            Self::Provider(provider) => provider.get_current_model().await,
        }
    }

    /// Get the provider name for logging
    pub fn provider_name(&self) -> &str {
        match self {
            Self::Whisper(_) => "Whisper (direct)",
            Self::Parakeet(_) => "Parakeet (direct)",
            Self::Provider(provider) => provider.provider_name(),
        }
    }
}

// ============================================================================
// MODEL VALIDATION AND INITIALIZATION
// ============================================================================

/// Validate that transcription models (Whisper or Parakeet) are ready before starting recording
pub async fn validate_transcription_model_ready<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    // Check transcript configuration to determine which engine to validate
    let config =
        match crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
            .await
        {
            Ok(Some(config)) => {
                info!(
                    "📝 Found transcript config - provider: {}, model: {}",
                    config.provider, config.model
                );
                config
            }
            Ok(None) => {
                info!("📝 No transcript config found, defaulting to parakeet");
                crate::api::api::TranscriptConfig {
                    provider: "parakeet".to_string(),
                    model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                    api_key: None,
                }
            }
            Err(e) => {
                warn!(
                    "⚠️ Failed to get transcript config: {}, defaulting to parakeet",
                    e
                );
                crate::api::api::TranscriptConfig {
                    provider: "parakeet".to_string(),
                    model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                    api_key: None,
                }
            }
        };

    // Validate based on provider
    match config.provider.as_str() {
        "localWhisper" => {
            info!("🔍 Validating Whisper model...");
            // Ensure whisper engine is initialized first
            if let Err(init_error) = crate::whisper_engine::commands::whisper_init().await {
                warn!("❌ Failed to initialize Whisper engine: {}", init_error);
                return Err(format!(
                    "Failed to initialize speech recognition: {}",
                    init_error
                ));
            }

            // Call the whisper validation command with config support
            match crate::whisper_engine::commands::whisper_validate_model_ready_with_config(app)
                .await
            {
                Ok(model_name) => {
                    info!(
                        "✅ Whisper model validation successful: {} is ready",
                        model_name
                    );
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ Whisper model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        "parakeet" => {
            info!("🔍 Validating Parakeet model...");
            // Ensure parakeet engine is initialized first
            if let Err(init_error) = crate::parakeet_engine::commands::parakeet_init().await {
                warn!("❌ Failed to initialize Parakeet engine: {}", init_error);
                return Err(format!(
                    "Failed to initialize Parakeet speech recognition: {}",
                    init_error
                ));
            }

            // Use the validation command that includes auto-discovery and loading
            // This matches the Whisper behavior for consistency
            match crate::parakeet_engine::commands::parakeet_validate_model_ready_with_config(app)
                .await
            {
                Ok(model_name) => {
                    info!(
                        "✅ Parakeet model validation successful: {} is ready",
                        model_name
                    );
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ Parakeet model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        AUDIO_CPP_PROVIDER => {
            info!("🔍 Validating audioCpp model...");
            match get_or_init_audio_cpp_provider(app).await {
                Ok(provider) => {
                    let model = provider
                        .get_current_model()
                        .await
                        .unwrap_or_else(|| "unknown".to_string());
                    info!(
                        "✅ audioCpp model validation successful: {} is ready",
                        model
                    );
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ audioCpp model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        TRANSCRIBE_CPP_PROVIDER => {
            info!("🔍 Validating transcribeCpp model...");
            match get_or_init_transcribe_cpp_provider(app).await {
                Ok(provider) => {
                    let model = provider
                        .get_current_model()
                        .await
                        .unwrap_or_else(|| "unknown".to_string());
                    info!(
                        "✅ transcribeCpp model validation successful: {} is ready",
                        model
                    );
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ transcribeCpp model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        other => {
            warn!(
                "❌ Unsupported transcription provider for local recording: {}",
                other
            );
            Err(format!(
                "Provider '{}' is not supported for local transcription. Please select 'localWhisper', 'parakeet', 'audioCpp' or 'transcribeCpp'.",
                other
            ))
        }
    }
}

/// Get or initialize the appropriate transcription engine based on provider configuration
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    // Get provider configuration from API
    let config =
        match crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
            .await
        {
            Ok(Some(config)) => {
                info!(
                    "📝 Transcript config - provider: {}, model: {}",
                    config.provider, config.model
                );
                config
            }
            Ok(None) => {
                info!("📝 No transcript config found, defaulting to parakeet");
                crate::api::api::TranscriptConfig {
                    provider: "parakeet".to_string(),
                    model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                    api_key: None,
                }
            }
            Err(e) => {
                warn!(
                    "⚠️ Failed to get transcript config: {}, defaulting to parakeet",
                    e
                );
                crate::api::api::TranscriptConfig {
                    provider: "parakeet".to_string(),
                    model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                    api_key: None,
                }
            }
        };

    // Initialize the appropriate engine based on provider
    match config.provider.as_str() {
        "parakeet" => {
            info!("🦜 Initializing Parakeet transcription engine");

            // Get Parakeet engine
            let engine = {
                let guard = crate::parakeet_engine::commands::PARAKEET_ENGINE
                    .lock()
                    .unwrap();
                guard.as_ref().cloned()
            };

            match engine {
                Some(engine) => {
                    // Check if model is loaded
                    if engine.is_model_loaded().await {
                        let model_name = engine
                            .get_current_model()
                            .await
                            .unwrap_or_else(|| "unknown".to_string());
                        info!("✅ Parakeet model '{}' already loaded", model_name);
                        Ok(TranscriptionEngine::Parakeet(engine))
                    } else {
                        Err("Parakeet engine initialized but no model loaded. This should not happen after validation.".to_string())
                    }
                }
                None => Err(
                    "Parakeet engine not initialized. This should not happen after validation."
                        .to_string(),
                ),
            }
        }
        AUDIO_CPP_PROVIDER => {
            info!("🎙️ Initializing audioCpp transcription engine");
            let provider = get_or_init_audio_cpp_provider(app).await?;
            Ok(TranscriptionEngine::Provider(provider))
        }
        TRANSCRIBE_CPP_PROVIDER => {
            info!("🧠 Initializing transcribeCpp transcription engine");
            let provider = get_or_init_transcribe_cpp_provider(app).await?;
            Ok(TranscriptionEngine::Provider(provider))
        }
        "localWhisper" | _ => {
            info!("🎤 Initializing Whisper transcription engine");
            let whisper_engine = get_or_init_whisper(app).await?;
            Ok(TranscriptionEngine::Whisper(whisper_engine))
        }
    }
}

/// Get or initialize the audioCpp transcription provider from the configured
/// model.
///
/// The provider is cached process-wide, so a second recording reuses the
/// already-loaded model. Failures are returned as user-facing messages: a
/// missing, downloading or corrupted model never reaches the native session
/// setup, and a native load/session error is reported instead of panicking.
pub async fn get_or_init_audio_cpp_provider<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<dyn TranscriptionProvider>, String> {
    get_or_init_audio_cpp_provider_with_model(app, None).await
}

/// Get or initialize the audioCpp provider, optionally for a specific model.
///
/// Batch jobs (retranscription, audio import) let the user pick any installed
/// audioCpp model in the dialog, which may differ from the configured one.
/// When `requested_model` is `Some`, that exact model is loaded — with no
/// fallback to another installed model, since silently transcribing with a
/// different model would defeat the comparison the user is making — and a
/// cached provider is reused only when it already runs that model.
pub async fn get_or_init_audio_cpp_provider_with_model<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<String>,
) -> Result<Arc<dyn TranscriptionProvider>, String> {
    if let Some(provider) = crate::audio_cpp_engine::transcription::provider() {
        if provider.is_model_loaded().await {
            let current = provider
                .get_current_model()
                .await
                .unwrap_or_else(|| "unknown".to_string());
            let reuse = match &requested_model {
                Some(requested) => current == *requested,
                None => true,
            };
            if reuse {
                info!("✅ audioCpp model '{}' already loaded, reusing", current);
                return Ok(provider);
            }
            info!(
                "🔄 Loaded audioCpp model '{}' does not match requested '{}', reloading",
                current,
                requested_model.as_deref().unwrap_or("")
            );
        }
    }

    // The transcript config supplies the model only when no explicit model
    // was requested; a batch job with an explicit model must not depend on
    // (or validate against) the configured provider.
    let configured_model = match requested_model {
        Some(_) => String::new(),
        None => match crate::api::api::api_get_transcript_config(
            app.clone(),
            app.clone().state(),
            None,
        )
        .await
        {
            Ok(Some(config)) if config.provider == AUDIO_CPP_PROVIDER => config.model,
            Ok(Some(config)) => {
                return Err(format!(
                    "Cannot initialize audioCpp: the transcript provider is '{}'. This is a bug in the transcription task initialization.",
                    config.provider
                ))
            }
            Ok(None) => String::new(),
            Err(e) => {
                warn!("⚠️ Failed to get transcript config for audioCpp: {}", e);
                String::new()
            }
        },
    };

    let manager = crate::audio_cpp_engine::commands::model_manager().ok_or_else(|| {
        "The audioCpp model manager is not initialized. Restart Samobranka and try again.".to_string()
    })?;

    let models = manager
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover audioCpp models: {}", e))?;

    let model = match &requested_model {
        Some(requested) => {
            crate::audio_cpp_engine::transcription::select_requested_transcription_model(
                &models, requested,
            )
        }
        None => crate::audio_cpp_engine::transcription::select_transcription_model(
            &models,
            &configured_model,
        ),
    }
    .map_err(|not_ready| {
        warn!("❌ audioCpp model not ready: {}", not_ready);
        not_ready.to_string()
    })?;

    info!(
        "Loading audioCpp model '{}' from {}",
        model.name,
        model.path.display()
    );

    // Library detection, registry creation and model load all block; keep
    // them off the async runtime (docs/research/audio-cpp-c-api.md).
    let load_result = tokio::task::spawn_blocking(move || {
        crate::audio_cpp_engine::transcription::load_native_provider(&model)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("audioCpp model load task failed: {}", e))?;

    match load_result {
        Ok(provider) => {
            info!(
                "✅ audioCpp provider ready: model '{}', family '{}'",
                provider.model_name(),
                provider.family()
            );
            let cached = crate::audio_cpp_engine::transcription::set_provider(provider);
            Ok(cached)
        }
        Err(message) => {
            warn!("❌ audioCpp model load failed: {}", message);
            Err(message)
        }
    }
}

/// Get or initialize the transcribeCpp transcription provider from the
/// configured model.
///
/// The provider is cached process-wide, so a second recording reuses the
/// already-loaded model. Failures are returned as user-facing messages: a
/// missing, downloading or corrupted model never reaches the native session
/// setup, and a native load/session error is reported instead of panicking.
pub async fn get_or_init_transcribe_cpp_provider<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<dyn TranscriptionProvider>, String> {
    get_or_init_transcribe_cpp_provider_with_model(app, None).await
}

/// Get or initialize the transcribeCpp provider, optionally for a specific
/// model.
///
/// Batch jobs (retranscription, audio import) let the user pick any installed
/// transcribe.cpp model in the dialog, which may differ from the configured
/// one. When `requested_model` is `Some`, that exact model is loaded — with
/// no fallback to another installed model, since silently transcribing with a
/// different model would defeat the comparison the user is making — and a
/// cached provider is reused only when it already runs that model.
pub async fn get_or_init_transcribe_cpp_provider_with_model<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<String>,
) -> Result<Arc<dyn TranscriptionProvider>, String> {
    if let Some(provider) = crate::transcribe_cpp_engine::transcription::provider() {
        if provider.is_model_loaded().await {
            let current = provider
                .get_current_model()
                .await
                .unwrap_or_else(|| "unknown".to_string());
            let reuse = match &requested_model {
                Some(requested) => current == *requested,
                None => true,
            };
            if reuse {
                info!("✅ transcribeCpp model '{}' already loaded, reusing", current);
                return Ok(provider);
            }
            info!(
                "🔄 Loaded transcribeCpp model '{}' does not match requested '{}', reloading",
                current,
                requested_model.as_deref().unwrap_or("")
            );
        }
    }

    // The transcript config supplies the model only when no explicit model
    // was requested; a batch job with an explicit model must not depend on
    // (or validate against) the configured provider. An empty configured
    // model (or no config row at all) uses the catalog default, mirroring
    // the api_get_transcript_config fallback so both agree on one model.
    let configured_model = match requested_model {
        Some(_) => String::new(),
        None => match crate::api::api::api_get_transcript_config(
            app.clone(),
            app.clone().state(),
            None,
        )
        .await
        {
            Ok(Some(config)) if config.provider == TRANSCRIBE_CPP_PROVIDER => {
                if config.model.is_empty() {
                    crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
                } else {
                    config.model
                }
            }
            Ok(Some(config)) => {
                return Err(format!(
                    "Cannot initialize transcribeCpp: the transcript provider is '{}'. This is a bug in the transcription task initialization.",
                    config.provider
                ))
            }
            Ok(None) => crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string(),
            Err(e) => {
                warn!("⚠️ Failed to get transcript config for transcribeCpp: {}", e);
                crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
            }
        },
    };

    let manager = crate::transcribe_cpp_engine::commands::model_manager().ok_or_else(|| {
        "The transcribe.cpp model manager is not initialized. Restart Samobranka and try again."
            .to_string()
    })?;

    let models = manager
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover transcribe.cpp models: {}", e))?;

    let model = match &requested_model {
        Some(requested) => {
            crate::transcribe_cpp_engine::transcription::select_requested_transcription_model(
                &models, requested,
            )
        }
        None => crate::transcribe_cpp_engine::transcription::select_transcription_model(
            &models,
            &configured_model,
        ),
    }
    .map_err(|not_ready| {
        warn!("❌ transcribeCpp model not ready: {}", not_ready);
        not_ready.to_string()
    })?;

    info!(
        "Loading transcribeCpp model '{}' from {}",
        model.name,
        model.path.display()
    );

    // Library discovery, the ABI gate, backend registration and the model
    // load all block; keep them off the async runtime (transcribe.h: every
    // entry point blocks).
    let load_result = tokio::task::spawn_blocking(move || {
        crate::transcribe_cpp_engine::transcription::load_native_provider(&model)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("transcribeCpp model load task failed: {}", e))?;

    match load_result {
        Ok(provider) => {
            info!(
                "✅ transcribeCpp provider ready: model '{}', backend '{}'",
                provider.model_name(),
                provider.backend()
            );
            let cached = crate::transcribe_cpp_engine::transcription::set_provider(provider);
            Ok(cached)
        }
        Err(message) => {
            warn!("❌ transcribeCpp model load failed: {}", message);
            Err(message)
        }
    }
}

/// Get or initialize transcription engine using API configuration
/// Returns Whisper engine if provider is localWhisper, otherwise returns error for non-Whisper providers
pub async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<crate::whisper_engine::WhisperEngine>, String> {
    // Check if engine already exists and has a model loaded
    let existing_engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard.as_ref().cloned()
    };

    if let Some(engine) = existing_engine {
        // Check if a model is already loaded
        if engine.is_model_loaded().await {
            let current_model = engine
                .get_current_model()
                .await
                .unwrap_or_else(|| "unknown".to_string());

            // NEW: Check if loaded model matches saved config
            let configured_model = match crate::api::api::api_get_transcript_config(
                app.clone(),
                app.clone().state(),
                None,
            )
            .await
            {
                Ok(Some(config)) => {
                    info!(
                        "📝 Saved transcript config - provider: {}, model: {}",
                        config.provider, config.model
                    );
                    if config.provider == "localWhisper" && !config.model.is_empty() {
                        Some(config.model)
                    } else {
                        None
                    }
                }
                Ok(None) => {
                    info!("📝 No transcript config found in database");
                    None
                }
                Err(e) => {
                    warn!("⚠️ Failed to get transcript config: {}", e);
                    None
                }
            };

            // If loaded model matches config, reuse it
            if let Some(ref expected_model) = configured_model {
                if current_model == *expected_model {
                    info!(
                        "✅ Loaded model '{}' matches saved config, reusing",
                        current_model
                    );
                    return Ok(engine);
                } else {
                    info!(
                        "🔄 Loaded model '{}' doesn't match saved config '{}', reloading correct model...",
                        current_model, expected_model
                    );
                    // Unload the incorrect model
                    engine.unload_model().await;
                    info!("📉 Unloaded incorrect model '{}'", current_model);
                    // Continue to model loading logic below
                }
            } else {
                // No specific config saved, accept currently loaded model
                info!(
                    "✅ No specific model configured, using currently loaded model: '{}'",
                    current_model
                );
                return Ok(engine);
            }
        } else {
            info!("🔄 Whisper engine exists but no model loaded, will load model from config");
        }
    }

    // Initialize new engine if needed
    info!("Initializing Whisper engine");

    // First ensure the engine is initialized
    if let Err(e) = crate::whisper_engine::commands::whisper_init().await {
        return Err(format!("Failed to initialize Whisper engine: {}", e));
    }

    // Get the engine reference
    let engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard
            .as_ref()
            .cloned()
            .ok_or("Failed to get initialized engine")?
    };

    // Get model configuration from API
    let model_to_load = match crate::api::api::api_get_transcript_config(
        app.clone(),
        app.clone().state(),
        None,
    )
    .await
    {
        Ok(Some(config)) => {
            info!(
                "Got transcript config from API - provider: {}, model: {}",
                config.provider, config.model
            );
            if config.provider == "localWhisper" {
                info!("Using model from API config: {}", config.model);
                config.model
            } else {
                // Non-Whisper provider (e.g., parakeet) - this function shouldn't be called
                return Err(format!(
                        "Cannot initialize Whisper engine: Config uses '{}' provider. This is a bug in the transcription task initialization.",
                        config.provider
                    ));
            }
        }
        Ok(None) => {
            info!("No transcript config found in API, falling back to 'small'");
            "small".to_string()
        }
        Err(e) => {
            warn!(
                "Failed to get transcript config from API: {}, falling back to 'small'",
                e
            );
            "small".to_string()
        }
    };

    info!("Selected model to load: {}", model_to_load);

    // Discover available models to check if the desired model is downloaded
    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover models: {}", e))?;

    info!("Discovered {} models", models.len());
    for model in &models {
        info!(
            "Model: {} - Status: {:?} - Path: {}",
            model.name,
            model.status,
            model.path.display()
        );
    }

    // Check if the desired model is available
    let model_info = models.iter().find(|model| model.name == model_to_load);

    if model_info.is_none() {
        info!(
            "Model '{}' not found in discovered models. Available models: {:?}",
            model_to_load,
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    match model_info {
        Some(model) => {
            match model.status {
                crate::whisper_engine::ModelStatus::Available => {
                    info!("Loading model: {}", model_to_load);
                    engine
                        .load_model(&model_to_load)
                        .await
                        .map_err(|e| format!("Failed to load model '{}': {}", model_to_load, e))?;
                    info!("✅ Model '{}' loaded successfully", model_to_load);
                }
                crate::whisper_engine::ModelStatus::Missing => {
                    return Err(format!(
                        "Model '{}' is not downloaded. Please download it first from the settings.",
                        model_to_load
                    ));
                }
                crate::whisper_engine::ModelStatus::Downloading { progress } => {
                    return Err(format!("Model '{}' is currently downloading ({}%). Please wait for it to complete.", model_to_load, progress));
                }
                crate::whisper_engine::ModelStatus::Error(ref err) => {
                    return Err(format!("Model '{}' has an error: {}. Please check the model or try downloading it again.", model_to_load, err));
                }
                crate::whisper_engine::ModelStatus::Corrupted { .. } => {
                    return Err(format!("Model '{}' is corrupted. Please delete it and download again from the settings.", model_to_load));
                }
            }
        }
        None => {
            // Check if we have any available models and try to load the first one
            let available_models: Vec<_> = models
                .iter()
                .filter(|m| matches!(m.status, crate::whisper_engine::ModelStatus::Available))
                .collect();

            if let Some(fallback_model) = available_models.first() {
                warn!(
                    "Model '{}' not found, falling back to available model: '{}'",
                    model_to_load, fallback_model.name
                );
                engine.load_model(&fallback_model.name).await.map_err(|e| {
                    format!(
                        "Failed to load fallback model '{}': {}",
                        fallback_model.name, e
                    )
                })?;
                info!(
                    "✅ Fallback model '{}' loaded successfully",
                    fallback_model.name
                );
            } else {
                return Err(format!("Model '{}' is not supported and no other models are available. Please download a model from the settings.", model_to_load));
            }
        }
    }

    Ok(engine)
}
