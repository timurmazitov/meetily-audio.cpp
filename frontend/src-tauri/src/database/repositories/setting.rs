use crate::audio::transcription::engine::{AUDIO_CPP_PROVIDER, TRANSCRIBE_CPP_PROVIDER};
use crate::database::models::{Setting, TranscriptSetting};
use crate::summary::CustomOpenAIConfig;
use sqlx::SqlitePool;

#[derive(serde::Deserialize, Debug)]
pub struct SaveModelConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "ollamaEndpoint")]
    pub ollama_endpoint: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SaveTranscriptConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

pub struct SettingsRepository;

// Transcript providers: transcribeCpp, deepgram, elevenLabs, groq, openai
// (localWhisper, parakeet and audioCpp were removed by the transcribe.cpp
// consolidation; see the soft migration in `normalize_transcript_provider`)
// Summary providers: openai, claude, ollama, groq, added openrouter
// NOTE: Handle data exclusion in the higher layer as this is database abstraction layer(using SELECT *)

/// Provider ids removed by the transcribe.cpp consolidation. A row saved by
/// an earlier app version may still name one; both the soft migration on
/// read and the save-time normalization use this single list.
const REMOVED_TRANSCRIPT_PROVIDERS: &[&str] = &["localWhisper", "parakeet", "audioCpp"];

/// Provider ids a transcript config save may persist: the transcribe.cpp
/// engine plus the cloud providers that still exist in code.
const VALID_TRANSCRIPT_PROVIDERS: &[&str] = &[
    TRANSCRIBE_CPP_PROVIDER,
    "deepgram",
    "elevenLabs",
    "groq",
    "openai",
];

/// Read-side soft migration for a stored (provider, model) pair.
///
/// A removed provider — `localWhisper`, `parakeet` or `audioCpp` — reads as
/// `transcribeCpp` with the Transcribe.CPP catalog default model, so a user
/// whose settings predate the consolidation gets a working app without any
/// action and without a schema migration (the stored row is never rewritten).
/// A `transcribeCpp` row that names no model also resolves to the catalog
/// default, keeping the dispatcher and the settings UI on one model.
fn normalize_transcript_provider(provider: String, model: String) -> (String, String) {
    if REMOVED_TRANSCRIPT_PROVIDERS.contains(&provider.as_str()) {
        return (
            TRANSCRIBE_CPP_PROVIDER.to_string(),
            crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string(),
        );
    }
    if provider == TRANSCRIBE_CPP_PROVIDER && model.trim().is_empty() {
        return (
            provider,
            crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string(),
        );
    }
    (provider, model)
}

impl SettingsRepository {
    pub async fn get_model_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<Setting>, sqlx::Error> {
        let setting = sqlx::query_as::<_, Setting>("SELECT * FROM settings LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(setting)
    }

    pub async fn save_model_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        whisper_model: &str,
        ollama_endpoint: Option<&str>,
    ) -> std::result::Result<(), sqlx::Error> {
        // Using id '1' for backward compatibility
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, ollamaEndpoint)
            VALUES ('1', $1, $2, $3, $4)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                whisperModel = excluded.whisperModel,
                ollamaEndpoint = excluded.ollamaEndpoint
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(whisper_model)
        .bind(ollama_endpoint)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn save_api_key(
        pool: &SqlitePool,
        provider: &str,
        api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config (customOpenAIConfig) instead of a separate API key column
        if provider == "custom-openai" {
            return Err(sqlx::Error::Protocol(
                "custom-openai provider should use save_custom_openai_config() instead of save_api_key()".into(),
            ));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "claude" => "anthropicApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(()), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, "{}")
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', 'large-v3', $1)
            ON CONFLICT(id) DO UPDATE SET
                "{}" = $1
            "#,
            api_key_column, api_key_column
        );
        sqlx::query(&query).bind(api_key).execute(pool).await?;

        Ok(())
    }

    pub async fn get_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        // Custom OpenAI uses JSON config - extract API key from there
        if provider == "custom-openai" {
            let config = Self::get_custom_openai_config(pool).await?;
            return Ok(config.and_then(|c| c.api_key));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(None), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "SELECT {} FROM settings WHERE id = '1' LIMIT 1",
            api_key_column
        );
        let api_key = sqlx::query_scalar(&query).fetch_optional(pool).await?;
        Ok(api_key)
    }

    pub async fn get_transcript_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<TranscriptSetting>, sqlx::Error> {
        let setting =
            sqlx::query_as::<_, TranscriptSetting>("SELECT * FROM transcript_settings LIMIT 1")
                .fetch_optional(pool)
                .await?;
        // Soft migration on read: legacy rows keep their bytes on disk, every
        // reader sees the transcribe.cpp default (see the helper docs).
        Ok(setting.map(|mut setting| {
            let (provider, model) =
                normalize_transcript_provider(setting.provider, setting.model);
            setting.provider = provider;
            setting.model = model;
            setting
        }))
    }

    pub async fn save_transcript_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        transcribe_backend: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Save-time half of the soft migration: a removed provider is stored
        // as the normalized transcribe.cpp default, so nothing new can land
        // in the table naming an engine that no longer exists. An unknown
        // provider is a caller error.
        let (provider, model) = if REMOVED_TRANSCRIPT_PROVIDERS.contains(&provider) {
            normalize_transcript_provider(provider.to_string(), model.to_string())
        } else if VALID_TRANSCRIPT_PROVIDERS.contains(&provider) {
            (provider.to_string(), model.to_string())
        } else {
            return Err(sqlx::Error::Configuration(
                format!("Invalid provider: {}", provider).into(),
            ));
        };
        sqlx::query(
            r#"
            INSERT INTO transcript_settings (id, provider, model, transcribeBackend)
            VALUES ('1', $1, $2, $3)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                transcribeBackend = excluded.transcribeBackend
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(transcribe_backend)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn save_transcript_api_key(
        pool: &SqlitePool,
        provider: &str,
        api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        let api_key_column = match provider {
            // Soft migration: the removed local engines never store an API
            // key; the old whisperApiKey special case is a no-op, exactly
            // like the cpp providers, and the legacy column stays untouched.
            "localWhisper" | "parakeet" => return Ok(()), // Local models run offline, no API key
            AUDIO_CPP_PROVIDER => return Ok(()), // audio.cpp models run locally, no API key
            TRANSCRIBE_CPP_PROVIDER => return Ok(()), // transcribe.cpp models run locally, no API key
            "deepgram" => "deepgramApiKey",
            "elevenLabs" => "elevenLabsApiKey",
            "groq" => "groqApiKey",
            "openai" => "openaiApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            r#"
            INSERT INTO transcript_settings (id, provider, model, "{}")
            VALUES ('1', $1, $2, $3)
            ON CONFLICT(id) DO UPDATE SET
                "{}" = $3
            "#,
            api_key_column,
            api_key_column
        );
        // A brand-new row starts from the transcribe.cpp default (the same
        // pair the soft migration normalizes to); the key save then updates
        // only its own column.
        sqlx::query(&query)
            .bind(TRANSCRIBE_CPP_PROVIDER)
            .bind(crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL)
            .bind(api_key)
            .execute(pool)
            .await?;

        Ok(())
    }

    pub async fn get_transcript_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        let api_key_column = match provider {
            // Soft migration: removed local engines have no API key; reading
            // one returns None instead of surfacing the legacy whisperApiKey
            // column (see the save counterpart).
            "localWhisper" | "parakeet" => return Ok(None), // Local models run offline, no API key
            AUDIO_CPP_PROVIDER => return Ok(None), // audio.cpp models run locally, no API key
            TRANSCRIBE_CPP_PROVIDER => return Ok(None), // transcribe.cpp models run locally, no API key
            "deepgram" => "deepgramApiKey",
            "elevenLabs" => "elevenLabsApiKey",
            "groq" => "groqApiKey",
            "openai" => "openaiApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "SELECT {} FROM transcript_settings WHERE id = '1' LIMIT 1",
            api_key_column
        );
        let api_key = sqlx::query_scalar(&query).fetch_optional(pool).await?;
        Ok(api_key)
    }

    pub async fn delete_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config - clear the entire config
        if provider == "custom-openai" {
            sqlx::query("UPDATE settings SET customOpenAIConfig = NULL WHERE id = '1'")
                .execute(pool)
                .await?;
            return Ok(());
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(()), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "UPDATE settings SET {} = NULL WHERE id = '1'",
            api_key_column
        );
        sqlx::query(&query).execute(pool).await?;

        Ok(())
    }

    // ===== SUMMARY SYSTEM PROMPT =====

    /// Gets the stored base system prompt for summary generation.
    ///
    /// # Returns
    /// * `Ok(Some(prompt))` - The stored text (possibly empty)
    /// * `Ok(None)` - No settings row exists
    /// * `Ok(Some(""))` - The column is NULL; callers treat blank as "use the default"
    pub async fn get_summary_system_prompt(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT summarySystemPrompt FROM settings WHERE id = '1' LIMIT 1")
            .fetch_optional(pool)
            .await
    }

    /// Saves the base system prompt for summary generation.
    pub async fn save_summary_system_prompt(
        pool: &SqlitePool,
        prompt: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, summarySystemPrompt)
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', 'large-v3', $1)
            ON CONFLICT(id) DO UPDATE SET
                summarySystemPrompt = excluded.summarySystemPrompt
            "#,
        )
        .bind(prompt)
        .execute(pool)
        .await?;

        Ok(())
    }

    // ===== CUSTOM OPENAI CONFIG METHODS =====
    /// Gets the custom OpenAI configuration from JSON
    ///
    /// # Returns
    /// * `Ok(Some(CustomOpenAIConfig))` - Config exists and is valid JSON
    /// * `Ok(None)` - No config stored
    /// * `Err(sqlx::Error)` - Database error
    pub async fn get_custom_openai_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<CustomOpenAIConfig>, sqlx::Error> {
        use sqlx::Row;

        let row = sqlx::query(
            r#"
            SELECT customOpenAIConfig
            FROM settings
            WHERE id = '1'
            LIMIT 1
            "#,
        )
        .fetch_optional(pool)
        .await?;

        match row {
            Some(record) => {
                let config_json: Option<String> = record.get("customOpenAIConfig");

                if let Some(json) = config_json {
                    // Parse JSON into CustomOpenAIConfig
                    let config: CustomOpenAIConfig = serde_json::from_str(&json).map_err(|e| {
                        sqlx::Error::Protocol(
                            format!("Invalid JSON in customOpenAIConfig: {}", e).into(),
                        )
                    })?;

                    Ok(Some(config))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Saves the custom OpenAI configuration as JSON
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - CustomOpenAIConfig to save (includes endpoint, apiKey, model, maxTokens, temperature, topP)
    ///
    /// # Returns
    /// * `Ok(())` - Config saved successfully
    /// * `Err(sqlx::Error)` - Database or JSON serialization error
    pub async fn save_custom_openai_config(
        pool: &SqlitePool,
        config: &CustomOpenAIConfig,
    ) -> std::result::Result<(), sqlx::Error> {
        // Serialize config to JSON
        let config_json = serde_json::to_string(config).map_err(|e| {
            sqlx::Error::Protocol(format!("Failed to serialize config to JSON: {}", e).into())
        })?;

        // Upsert into settings table
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, customOpenAIConfig)
            VALUES ('1', 'custom-openai', $1, 'large-v3', $2)
            ON CONFLICT(id) DO UPDATE SET
                customOpenAIConfig = excluded.customOpenAIConfig
            "#,
        )
        .bind(&config.model)
        .bind(config_json)
        .execute(pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            r#"CREATE TABLE transcript_settings (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                whisperApiKey TEXT,
                deepgramApiKey TEXT,
                elevenLabsApiKey TEXT,
                groqApiKey TEXT,
                openaiApiKey TEXT,
                transcribeBackend TEXT NOT NULL DEFAULT 'auto'
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn saved_provider(pool: &SqlitePool) -> (String, String) {
        let setting = SettingsRepository::get_transcript_config(pool)
            .await
            .unwrap()
            .expect("transcript config saved");
        (setting.provider, setting.model)
    }

    async fn saved_backend(pool: &SqlitePool) -> Option<String> {
        SettingsRepository::get_transcript_config(pool)
            .await
            .unwrap()
            .expect("transcript config saved")
            .transcribe_backend
    }

    #[tokio::test]
    async fn the_compute_backend_round_trips_and_absent_means_auto() {
        // The column is NOT NULL DEFAULT 'auto': a save without an explicit
        // mode (an older caller or a cloud-provider save) stores 'auto', and
        // a save with a mode survives a "restart" (a fresh read).
        let pool = test_pool().await;
        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "parakeet-ultra-Q8_0", "auto")
            .await
            .unwrap();
        assert_eq!(saved_backend(&pool).await.as_deref(), Some("auto"));

        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "parakeet-ultra-Q8_0", "vulkan")
            .await
            .unwrap();
        assert_eq!(saved_backend(&pool).await.as_deref(), Some("vulkan"));
        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                "parakeet-ultra-Q8_0".to_string()
            ),
            "provider and model are untouched by the mode change"
        );

        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "gigaam-v3-ctc-Q8_0", "cpu")
            .await
            .unwrap();
        assert_eq!(saved_backend(&pool).await.as_deref(), Some("cpu"));

        // A row predating the selector (inserted without the column) reads
        // back as the schema default.
        sqlx::query("INSERT INTO transcript_settings (id, provider, model) VALUES ('2', 'parakeet', 'm')")
            .execute(&pool)
            .await
            .unwrap();
        let row: (String,) =
            sqlx::query_as("SELECT transcribeBackend FROM transcript_settings WHERE id = '2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "auto");
    }

    #[tokio::test]
    async fn removed_providers_written_through_a_save_read_back_as_transcribe_cpp_default() {
        // Soft migration: an old client (or any caller) that still names a
        // removed provider lands on transcribe.cpp with the catalog default
        // model, so the app works unchanged after the update.
        let pool = test_pool().await;

        for (legacy_provider, legacy_model) in [
            ("localWhisper", "large-v3-turbo"),
            ("parakeet", "parakeet-tdt-0.6b-v3-int8"),
            ("audioCpp", "gigaam-v3-e2e-ctc-f16"),
        ] {
            SettingsRepository::save_transcript_config(&pool, legacy_provider, legacy_model, "auto")
                .await
                .unwrap();

            assert_eq!(
                saved_provider(&pool).await,
                (
                    "transcribeCpp".to_string(),
                    crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
                ),
                "legacy provider {legacy_provider} normalizes to the transcribe.cpp default"
            );
        }
    }

    #[tokio::test]
    async fn legacy_rows_written_before_the_migration_read_back_as_transcribe_cpp_default() {
        // A database written by a pre-migration app: rows land in the table
        // directly, naming a removed provider. Every read — what the app does
        // on launch — sees the transcribe.cpp default. The DB itself is
        // never rewritten: no migration, the legacy values stay on disk.
        let pool = test_pool().await;

        for (legacy_provider, legacy_model) in [
            ("localWhisper", "large-v3-turbo"),
            ("parakeet", "parakeet-tdt-0.6b-v3-int8"),
            ("audioCpp", "gigaam-v3-e2e-ctc-f16"),
        ] {
            sqlx::query(
                "INSERT OR REPLACE INTO transcript_settings (id, provider, model) VALUES ('1', ?, ?)",
            )
            .bind(legacy_provider)
            .bind(legacy_model)
            .execute(&pool)
            .await
            .unwrap();

            assert_eq!(
                saved_provider(&pool).await,
                (
                    "transcribeCpp".to_string(),
                    crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
                ),
                "legacy row {legacy_provider}/{legacy_model} reads back normalized"
            );

            let raw: (String, String) = sqlx::query_as(
                "SELECT provider, model FROM transcript_settings WHERE id = '1'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                raw,
                (legacy_provider.to_string(), legacy_model.to_string()),
                "normalization is read-only: the stored row is untouched"
            );
        }
    }

    #[tokio::test]
    async fn a_transcribe_cpp_row_with_a_missing_model_reads_back_the_catalog_default() {
        // A row predating the model being saved must not strand the user on
        // an empty model: every read resolves to the catalog default.
        let pool = test_pool().await;
        sqlx::query("INSERT INTO transcript_settings (id, provider, model) VALUES ('1', 'transcribeCpp', '')")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
            )
        );
    }

    #[tokio::test]
    async fn saving_an_unknown_provider_is_rejected() {
        // Only transcribe.cpp and the cloud providers may be saved; anything
        // else is a caller error, not a silent default.
        let pool = test_pool().await;

        assert!(
            SettingsRepository::save_transcript_config(&pool, "not-a-provider", "m", "auto")
                .await
                .is_err()
        );
        assert!(
            sqlx::query("SELECT 1 FROM transcript_settings WHERE id = '1'")
                .fetch_optional(&pool)
                .await
                .unwrap()
                .is_none(),
            "the rejected save stored nothing"
        );
    }

    #[tokio::test]
    async fn transcribe_cpp_and_cloud_providers_round_trip_untouched() {
        // Normalization must not disturb the valid provider set: transcribe.cpp
        // keeps its chosen model and the clouds keep their ids.
        let pool = test_pool().await;

        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "gigaam-v3-ctc-Q8_0", "vulkan")
            .await
            .unwrap();
        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                "gigaam-v3-ctc-Q8_0".to_string()
            )
        );

        for cloud in ["deepgram", "elevenLabs", "groq", "openai"] {
            SettingsRepository::save_transcript_config(&pool, cloud, "a-model", "auto")
                .await
                .unwrap();
            assert_eq!(
                saved_provider(&pool).await,
                (cloud.to_string(), "a-model".to_string())
            );
        }
    }

    #[tokio::test]
    async fn a_cloud_api_key_on_an_empty_table_never_seeds_a_removed_provider() {
        // The cloud-key INSERT carries a placeholder row for a missing
        // config; the placeholder must not name a removed engine.
        let pool = test_pool().await;

        SettingsRepository::save_transcript_api_key(&pool, "deepgram", "dg-key")
            .await
            .unwrap();

        let raw: (String, String) =
            sqlx::query_as("SELECT provider, model FROM transcript_settings WHERE id = '1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            !REMOVED_TRANSCRIPT_PROVIDERS.contains(&raw.0.as_str()),
            "the seeded row must not name a removed provider, got {:?}",
            raw
        );
        // The key itself round-trips.
        assert_eq!(
            SettingsRepository::get_transcript_api_key(&pool, "deepgram")
                .await
                .unwrap()
                .as_deref(),
            Some("dg-key")
        );
    }

    #[tokio::test]
    async fn transcribe_cpp_api_key_is_never_read_or_written_under_a_legacy_name() {
        // The special whisperApiKey handling is out of the local path: an
        // old caller saving a key under "localWhisper" stores nothing and
        // reads nothing back, exactly like the cpp providers.
        let pool = test_pool().await;
        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "parakeet-ultra-Q8_0", "auto")
            .await
            .unwrap();

        SettingsRepository::save_transcript_api_key(&pool, "localWhisper", "super-secret")
            .await
            .unwrap();

        assert_eq!(
            SettingsRepository::get_transcript_api_key(&pool, "localWhisper")
                .await
                .unwrap(),
            None
        );
        let stored: Option<String> = sqlx::query_scalar("SELECT whisperApiKey FROM transcript_settings WHERE id = '1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, None, "the legacy whisperApiKey column stays untouched");
    }

    #[tokio::test]
    async fn audiocpp_provider_and_model_read_back_as_transcribe_cpp_after_a_restart() {
        // The pre-migration flow saved audio.cpp with a GigaAM model; after
        // the update the same saved state reads back as transcribe.cpp with
        // the catalog default (the user story: a working app, no actions).
        let pool = test_pool().await;

        SettingsRepository::save_transcript_config(&pool, "audioCpp", "gigaam-v3-e2e-ctc-f16", "auto")
            .await
            .unwrap();

        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
            )
        );
    }

    #[tokio::test]
    async fn switching_providers_persists_the_new_provider() {
        // Legacy saves normalize; switching between valid providers keeps
        // each one's id and model.
        let pool = test_pool().await;

        SettingsRepository::save_transcript_config(&pool, "audioCpp", "gigaam-v3-e2e-ctc-f16", "auto")
            .await
            .unwrap();
        SettingsRepository::save_transcript_config(&pool, "localWhisper", "large-v3-turbo", "auto")
            .await
            .unwrap();
        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
            ),
            "both legacy saves land on the transcribe.cpp default"
        );

        SettingsRepository::save_transcript_config(&pool, "deepgram", "nova-2-phonecall", "auto")
            .await
            .unwrap();
        assert_eq!(
            saved_provider(&pool).await,
            ("deepgram".to_string(), "nova-2-phonecall".to_string())
        );
    }

    #[tokio::test]
    async fn audiocpp_needs_no_api_key_and_saving_one_is_a_no_op() {
        let pool = test_pool().await;
        SettingsRepository::save_transcript_config(&pool, "audioCpp", "gigaam-v3-e2e-ctc-f16", "auto")
            .await
            .unwrap();

        // Neither call may fail with "Invalid provider": the settings modal
        // saves the provider and an optional key together.
        SettingsRepository::save_transcript_api_key(&pool, "audioCpp", "ignored")
            .await
            .unwrap();

        assert_eq!(
            SettingsRepository::get_transcript_api_key(&pool, "audioCpp")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL.to_string()
            ),
            "the legacy provider save normalized like any other removed provider"
        );
    }

    #[tokio::test]
    async fn transcribecpp_needs_no_api_key_and_saving_one_is_a_no_op() {
        let pool = test_pool().await;
        SettingsRepository::save_transcript_config(&pool, "transcribeCpp", "parakeet-ultra-Q8_0", "auto")
            .await
            .unwrap();

        // Neither call may fail with "Invalid provider": the settings modal
        // saves the provider and an optional key together.
        SettingsRepository::save_transcript_api_key(&pool, "transcribeCpp", "ignored")
            .await
            .unwrap();

        assert_eq!(
            SettingsRepository::get_transcript_api_key(&pool, "transcribeCpp")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            saved_provider(&pool).await,
            (
                "transcribeCpp".to_string(),
                "parakeet-ultra-Q8_0".to_string()
            )
        );
    }

    #[tokio::test]
    async fn other_providers_keep_their_api_key_behaviour() {
        let pool = test_pool().await;
        SettingsRepository::save_transcript_config(&pool, "parakeet", "parakeet-tdt-0.6b-v3-int8", "auto")
            .await
            .unwrap();

        SettingsRepository::save_transcript_api_key(&pool, "parakeet", "ignored")
            .await
            .unwrap();
        assert_eq!(
            SettingsRepository::get_transcript_api_key(&pool, "parakeet")
                .await
                .unwrap(),
            None
        );

        assert!(
            SettingsRepository::get_transcript_api_key(&pool, "not-a-provider")
                .await
                .is_err()
        );
    }
}
