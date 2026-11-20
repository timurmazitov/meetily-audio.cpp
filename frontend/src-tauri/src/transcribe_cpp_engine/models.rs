//! Model management for the transcribe.cpp provider: discovery, download,
//! cancellation and deletion of the catalogued artifacts.
//!
//! This module owns only model *files* and their on-disk state. It never
//! loads the native library and never starts a session, so the catalog works
//! before transcription support exists (and keeps working without it).
//! Downloads go through the shared [`crate::model_downloader`] mechanism
//! (HTTP streaming with progress, cooperative cancellation), while the
//! transcribe.cpp specific parts — the catalog, the pinned exact artifact
//! sizes and SHA-256 digests, and the per-model family metadata — live here.
//!
//! State is derived from disk: a model is available when its artifact is
//! present with the exact catalogued size, a valid GGUF header, **and** the
//! pinned SHA-256 digest. The digest check is stronger than the audio.cpp
//! manager, which stops at size plus magic. Nothing is cached in a settings
//! file, so the state naturally survives restarts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::{watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::catalog::{self, ModelInfo, ModelSpec, ModelStatus};

pub use crate::model_downloader::{is_download_cancelled, DownloadCancelled};

/// Outcome of a cancellation request. Mirrors the Whisper, Parakeet and
/// audio.cpp managers so the frontend can share its cancellation handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CancelDownloadOutcome {
    Cancelled,
    Pending,
}

/// How long a cancellation waits for the worker to clean up before
/// reporting `Pending`.
const CANCEL_DOWNLOAD_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Size of the streaming hash buffer.
const SHA256_READ_CHUNK: usize = 64 * 1024;

struct ActiveDownload {
    cancellation: CancellationToken,
    completion: watch::Sender<bool>,
}

/// Hex SHA-256 of a file, streamed in chunks so a multi-gigabyte artifact
/// never lands fully in memory.
async fn sha256_of_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|error| anyhow!("Failed to open model file for hashing: {error}"))?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; SHA256_READ_CHUNK];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| anyhow!("Failed to hash model file: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Check that a file carries the pinned SHA-256 digest.
async fn validate_sha256(path: &Path, expected_sha256: &str) -> Result<()> {
    let actual = sha256_of_file(path).await?;
    if actual != expected_sha256 {
        return Err(anyhow!(
            "Model file has SHA-256 {actual}, expected {expected_sha256}"
        ));
    }
    Ok(())
}

/// Validate an artifact against the catalog: it must carry a known GGML/GGUF
/// magic number, be exactly `expected_bytes` long, and hash to
/// `expected_sha256`.
///
/// Exact-size plus pinned-digest validation is stronger than the shared
/// minimum-size check and is what the pinned HuggingFace revisions allow: the
/// catalogued artifact is a single immutable file, so a size or digest
/// mismatch means a truncated, resumed, tampered or replaced download.
pub async fn validate_artifact(
    model_path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<()> {
    crate::model_downloader::validate_downloaded_model(model_path, expected_bytes).await?;

    let metadata = fs::metadata(model_path)
        .await
        .map_err(|error| anyhow!("Failed to read downloaded model file metadata: {error}"))?;
    if metadata.len() != expected_bytes {
        return Err(anyhow!(
            "Downloaded model file has {} bytes, expected exactly {} bytes",
            metadata.len(),
            expected_bytes
        ));
    }

    validate_sha256(model_path, expected_sha256).await
}

/// Ownership of the transcribe.cpp model files.
pub struct TranscribeCppModelManager {
    models_dir: PathBuf,
    catalog: &'static [ModelSpec],
    available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    active_downloads: Arc<Mutex<HashMap<String, Arc<ActiveDownload>>>>,
}

impl TranscribeCppModelManager {
    /// Create a manager rooted at `<models>/transcribecpp` with the
    /// application model catalog.
    pub fn new(models_root: Option<PathBuf>) -> Result<Self> {
        Self::with_catalog(models_root, catalog::catalog())
    }

    /// Create a manager for an explicit catalog. Production always passes
    /// [`catalog::catalog`]; tests use small specs to exercise the download
    /// lifecycle without multi-hundred-megabyte fixtures.
    pub fn with_catalog(
        models_root: Option<PathBuf>,
        catalog: &'static [ModelSpec],
    ) -> Result<Self> {
        let models_dir = match models_root {
            Some(root) => root.join("transcribecpp"),
            None => std::env::current_dir()
                .map_err(|error| anyhow!("Failed to get current directory: {error}"))?
                .join("models")
                .join("transcribecpp"),
        };

        if !models_dir.exists() {
            std::fs::create_dir_all(&models_dir).map_err(|error| {
                anyhow!(
                    "Failed to create transcribe.cpp models directory {}: {}",
                    models_dir.display(),
                    error
                )
            })?;
        }

        Ok(Self {
            models_dir,
            catalog,
            available_models: Arc::new(RwLock::new(HashMap::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// The directory holding the transcribe.cpp artifacts.
    pub fn models_directory(&self) -> &Path {
        &self.models_dir
    }

    fn spec(&self, model_name: &str) -> Result<&'static ModelSpec> {
        self.catalog
            .iter()
            .find(|spec| spec.name == model_name)
            .ok_or_else(|| anyhow!("Unknown transcribe.cpp model: {model_name}"))
    }

    /// Catalogued models with their current installation state.
    ///
    /// Status is read from disk on every call so that a restart, a manual
    /// file removal or a partially deleted artifact is always reflected.
    pub async fn discover_models(&self) -> Result<Vec<ModelInfo>> {
        // Cache progress first, then read the disk without holding the state
        // locks across file I/O: a slow/virus-scanned file must not stall
        // progress reporting or a concurrent download.
        let in_flight: HashMap<String, u8> = {
            let active_downloads = self.active_downloads.lock().await;
            let cached = self.available_models.read().await;
            self.catalog
                .iter()
                .filter(|spec| active_downloads.contains_key(spec.name))
                .map(|spec| {
                    let progress = cached
                        .get(spec.name)
                        .and_then(|model| match model.status {
                            ModelStatus::Downloading { progress } => Some(progress),
                            _ => None,
                        })
                        .unwrap_or(0);
                    (spec.name.to_string(), progress)
                })
                .collect()
        };

        let mut models = Vec::with_capacity(self.catalog.len());
        for spec in self.catalog {
            let status = match in_flight.get(spec.name) {
                Some(progress) => ModelStatus::Downloading {
                    progress: *progress,
                },
                None => self.disk_status(spec).await,
            };
            models.push(spec.model_info(&self.models_dir, status));
        }

        let mut available_models = self.available_models.write().await;
        available_models.clear();
        for model in &models {
            available_models.insert(model.name.clone(), model.clone());
        }

        Ok(models)
    }

    /// The status of one catalogued model as recorded on disk. A file with
    /// the catalogued size but a wrong digest or header is `Corrupted`, never
    /// `Available` — a corrupt artifact must not reach the native loader.
    async fn disk_status(&self, spec: &ModelSpec) -> ModelStatus {
        let path = spec.file_path(&self.models_dir);
        let metadata = match fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(_) => return ModelStatus::Missing,
        };

        let file_size = metadata.len();
        if file_size == 0 {
            return ModelStatus::Missing;
        }
        if file_size != spec.exact_bytes {
            return ModelStatus::Corrupted {
                file_size,
                expected_min_size: spec.exact_bytes,
            };
        }

        if let Err(error) = crate::model_downloader::validate_model_magic(&path).await {
            log::warn!(
                "transcribe.cpp model {} has the catalogued size but failed header validation: {}",
                spec.name,
                error
            );
            return ModelStatus::Corrupted {
                file_size,
                expected_min_size: spec.exact_bytes,
            };
        }

        match validate_sha256(&path, spec.sha256).await {
            Ok(()) => ModelStatus::Available,
            Err(error) => {
                log::warn!(
                    "transcribe.cpp model {} has the catalogued size but failed digest validation: {}",
                    spec.name,
                    error
                );
                ModelStatus::Corrupted {
                    file_size,
                    expected_min_size: spec.exact_bytes,
                }
            }
        }
    }

    /// Delete a downloaded artifact. Deleting a model that is not present is
    /// not an error: the end state is the same.
    pub async fn delete_model(&self, model_name: &str) -> Result<String> {
        let spec = self.spec(model_name)?;

        if self.active_downloads.lock().await.contains_key(model_name) {
            return Err(anyhow!(
                "Model {model_name} is currently downloading; cancel the download before deleting it"
            ));
        }

        let path = spec.file_path(&self.models_dir);
        match fs::remove_file(&path).await {
            Ok(()) => log::info!("Deleted transcribe.cpp model file: {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                log::info!("transcribe.cpp model {} was already absent", model_name);
            }
            Err(error) => {
                return Err(anyhow!("Failed to delete {}: {}", path.display(), error));
            }
        }

        let mut models = self.available_models.write().await;
        if let Some(model) = models.get_mut(model_name) {
            model.status = ModelStatus::Missing;
        }

        Ok(format!("Deleted transcribe.cpp model '{model_name}'"))
    }

    /// Whether the model's artifact is installed and valid right now.
    pub async fn is_model_available(&self, model_name: &str) -> bool {
        match self.spec(model_name) {
            Ok(spec) => matches!(self.disk_status(spec).await, ModelStatus::Available),
            Err(_) => false,
        }
    }

    /// Download one artifact with progress reporting and cooperative
    /// cancellation.
    ///
    /// `url` and `expected_sha256` are parameters rather than being read from
    /// the catalog so the same code path can serve a mirror, and so the
    /// download behaviour can be exercised against a local server in tests.
    pub async fn download_artifact(
        &self,
        model_name: &str,
        filename: &str,
        url: &str,
        expected_bytes: u64,
        expected_sha256: &str,
        progress: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        // Artifacts live directly in the models directory; a name with a path
        // separator would escape it.
        if filename.is_empty()
            || filename.contains('/')
            || filename.contains('\\')
            || Path::new(filename)
                .file_name()
                .and_then(|name| name.to_str())
                != Some(filename)
        {
            return Err(anyhow!("Invalid model filename: {filename}"));
        }

        let active_download = self.reserve_active_download(model_name).await?;
        let file_path = self.models_dir.join(filename);

        let result = self
            .download_with_owner(model_name, url, &file_path, &active_download, progress)
            .await;

        self.finish_download(
            model_name,
            expected_bytes,
            expected_sha256,
            &active_download,
            &file_path,
            result,
        )
        .await
    }

    /// Download a catalogued model from its pinned upstream revision.
    pub async fn download_model(
        &self,
        model_name: &str,
        progress: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        let spec = self.spec(model_name)?;

        self.download_artifact(
            spec.name,
            spec.filename,
            spec.source_url,
            spec.exact_bytes,
            spec.sha256,
            progress,
        )
        .await
    }

    async fn reserve_active_download(&self, model_name: &str) -> Result<Arc<ActiveDownload>> {
        let mut active_downloads = self.active_downloads.lock().await;
        if active_downloads.contains_key(model_name) {
            return Err(anyhow!(
                "Download already in progress for model: {model_name}"
            ));
        }

        let (completion, _) = watch::channel(false);
        let active_download = Arc::new(ActiveDownload {
            cancellation: CancellationToken::new(),
            completion,
        });
        active_downloads.insert(model_name.to_string(), Arc::clone(&active_download));
        Ok(active_download)
    }

    async fn set_status(&self, model_name: &str, status: ModelStatus) {
        let mut models = self.available_models.write().await;
        if let Some(model) = models.get_mut(model_name) {
            model.status = status;
        }
    }

    async fn download_with_owner(
        &self,
        model_name: &str,
        url: &str,
        file_path: &Path,
        active_download: &ActiveDownload,
        progress: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        if active_download.cancellation.is_cancelled() {
            return Err(DownloadCancelled.into());
        }

        fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|error| anyhow!("Failed to create transcribe.cpp models directory: {error}"))?;

        self.set_status(model_name, ModelStatus::Downloading { progress: 0 })
            .await;

        let available_models = Arc::clone(&self.available_models);
        let model_name_owned = model_name.to_string();
        let progress = Arc::new(std::sync::Mutex::new(progress));

        crate::model_downloader::download_file(
            url,
            file_path,
            &active_download.cancellation,
            move |reported| {
                let available_models = Arc::clone(&available_models);
                let model_name = model_name_owned.clone();
                let progress = Arc::clone(&progress);
                async move {
                    {
                        let mut models = available_models.write().await;
                        if let Some(model) = models.get_mut(&model_name) {
                            model.status = ModelStatus::Downloading { progress: reported };
                        }
                    }
                    if let Ok(callback) = progress.lock() {
                        if let Some(callback) = callback.as_ref() {
                            callback(reported);
                        }
                    }
                }
            },
        )
        .await
    }

    async fn finish_download(
        &self,
        model_name: &str,
        expected_bytes: u64,
        expected_sha256: &str,
        active_download: &Arc<ActiveDownload>,
        file_path: &Path,
        mut result: Result<()>,
    ) -> Result<()> {
        let owner_matches = {
            let active_downloads = self.active_downloads.lock().await;
            matches!(
                active_downloads.get(model_name),
                Some(current) if Arc::ptr_eq(current, active_download)
            )
        };
        if !owner_matches {
            log::warn!(
                "transcribe.cpp download owner for {model_name} was no longer active during finalization"
            );
            active_download.completion.send_replace(true);
            return result;
        }

        if result.is_ok() && !active_download.cancellation.is_cancelled() {
            result = validate_artifact(file_path, expected_bytes, expected_sha256).await;
            if let Err(ref error) = result {
                log::warn!("transcribe.cpp model {model_name} failed validation: {error}");
            }
        }

        let cancellation_won = active_download.cancellation.is_cancelled()
            || result.as_ref().err().is_some_and(is_download_cancelled);

        // A cancelled or failed transfer never leaves a file behind: the next
        // attempt must start from a clean state.
        if (cancellation_won || result.is_err()) && file_path.exists() {
            if let Err(error) = fs::remove_file(file_path).await {
                log::warn!(
                    "Failed to clean up incomplete transcribe.cpp model file {}: {}",
                    file_path.display(),
                    error
                );
            }
        }

        {
            let mut active_downloads = self.active_downloads.lock().await;
            active_downloads.remove(model_name);

            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = if cancellation_won || result.is_err() {
                    ModelStatus::Missing
                } else {
                    ModelStatus::Available
                };
                model.path = file_path.to_path_buf();
                model.size_bytes = expected_bytes;
            }
        }
        active_download.completion.send_replace(true);

        if cancellation_won {
            return Err(DownloadCancelled.into());
        }

        match result {
            Ok(()) => {
                log::info!("transcribe.cpp model downloaded: {model_name}");
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Cancel an ongoing download and wait for its cleanup.
    pub async fn cancel_download(&self, model_name: &str) -> Result<CancelDownloadOutcome> {
        self.cancel_download_with_timeout(model_name, CANCEL_DOWNLOAD_CLEANUP_TIMEOUT)
            .await
    }

    async fn cancel_download_with_timeout(
        &self,
        model_name: &str,
        cleanup_timeout: Duration,
    ) -> Result<CancelDownloadOutcome> {
        let active_download = {
            let active_downloads = self.active_downloads.lock().await;
            let Some(active_download) = active_downloads.get(model_name).cloned() else {
                return Ok(CancelDownloadOutcome::Cancelled);
            };
            active_download.cancellation.cancel();
            active_download
        };

        let mut completion = active_download.completion.subscribe();
        if !*completion.borrow() {
            match tokio::time::timeout(cleanup_timeout, completion.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    return Err(anyhow!(
                        "Download worker ended before completing cancellation cleanup"
                    ));
                }
                Err(_) => return Ok(CancelDownloadOutcome::Pending),
            }
        }

        Ok(CancelDownloadOutcome::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use crate::transcribe_cpp_engine::catalog::ModelSpec;

    /// A tiny catalogued model whose artifact is `GGUF` + zero padding.
    fn test_payload(size: usize) -> Vec<u8> {
        let mut payload = b"GGUF".to_vec();
        payload.resize(size, 0);
        payload
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn test_catalog(payload: &'static [u8]) -> &'static [ModelSpec] {
        Box::leak(
            vec![ModelSpec {
                name: "test-ultra-q8_0",
                display_name: "Test Ultra",
                family: "parakeet-ultra",
                quantization: "Q8_0",
                filename: "test-ultra-q8_0.gguf",
                size_mb: (payload.len() as u64 / (1024 * 1024)) as u32,
                exact_bytes: payload.len() as u64,
                description: "test model",
                sha256: Box::leak(sha256_hex(payload).into_boxed_str()),
                source_url: "http://127.0.0.1/model.gguf",
            }]
            .into_boxed_slice(),
        )
    }

    fn test_payload_static(size: usize) -> &'static [u8] {
        Box::leak(test_payload(size).into_boxed_slice())
    }

    fn manager(dir: &std::path::Path, payload: &'static [u8]) -> TranscribeCppModelManager {
        TranscribeCppModelManager::with_catalog(Some(dir.to_path_buf()), test_catalog(payload))
            .unwrap()
    }

    fn artifact_path(dir: &std::path::Path) -> PathBuf {
        dir.join("transcribecpp").join("test-ultra-q8_0.gguf")
    }

    /// Wait (bounded) for the downloader to create its partial file, so the
    /// cancellation cleanup assertion is not vacuous.
    async fn wait_for_file(path: &Path) {
        for _ in 0..200 {
            if path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("download never created the partial file {}", path.display());
    }

    async fn serve_body(body: &'static [u8]) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await;
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.flush().await.unwrap();
        });
        (format!("http://{address}/model.gguf"), server)
    }

    async fn serve_stalled() -> (
        String,
        oneshot::Receiver<()>,
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (headers_sent, headers_ready) = oneshot::channel();
        let (release_body, body_released) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = socket.read(&mut request).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            socket.flush().await.unwrap();
            headers_sent.send(()).unwrap();
            let _ = body_released.await;
            let _ = socket.write_all(b"GGUF").await;
        });
        (
            format!("http://{address}/model.gguf"),
            headers_ready,
            release_body,
            server,
        )
    }

    /// Download helper that pulls in the catalog digest, mirroring
    /// `download_model` but with a caller-supplied URL.
    async fn download_from(
        manager: &TranscribeCppModelManager,
        url: &str,
        expected_bytes: u64,
        expected_sha256: &str,
    ) -> Result<()> {
        manager
            .download_artifact(
                "test-ultra-q8_0",
                "test-ultra-q8_0.gguf",
                url,
                expected_bytes,
                expected_sha256,
                None,
            )
            .await
    }

    #[tokio::test]
    async fn discover_reports_missing_before_download() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);

        let models = manager.discover_models().await.unwrap();

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "test-ultra-q8_0");
        assert_eq!(models[0].family, "parakeet-ultra");
        assert_eq!(models[0].quantization, "Q8_0");
        assert!(matches!(models[0].status, ModelStatus::Missing));
        assert!(!manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn discover_reports_available_for_a_valid_complete_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        std::fs::write(artifact_path(dir.path()), payload).unwrap();

        let models = manager.discover_models().await.unwrap();

        assert!(matches!(models[0].status, ModelStatus::Available));
        assert!(manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn discover_rejects_a_wrong_size_artifact_as_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        std::fs::write(artifact_path(dir.path()), test_payload(16)).unwrap();

        let models = manager.discover_models().await.unwrap();

        match &models[0].status {
            ModelStatus::Corrupted {
                file_size,
                expected_min_size,
            } => {
                assert_eq!(*file_size, 16);
                assert_eq!(*expected_min_size, 1024);
            }
            other => panic!("expected Corrupted, got {other:?}"),
        }
        assert!(!manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn discover_rejects_a_correctly_sized_file_without_a_gguf_header() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        std::fs::write(artifact_path(dir.path()), vec![0_u8; 1024]).unwrap();

        let models = manager.discover_models().await.unwrap();

        assert!(matches!(models[0].status, ModelStatus::Corrupted { .. }));
        assert!(!manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn discover_rejects_a_correctly_sized_file_with_the_wrong_digest() {
        // Same size and GGUF magic, different content: only the pinned
        // SHA-256 can catch this, so it must classify as Corrupted.
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        let mut tampered = test_payload(1024);
        let last = tampered.len() - 1;
        tampered[last] = 0xAB;
        assert_eq!(tampered.len(), payload.len());
        std::fs::write(artifact_path(dir.path()), &tampered).unwrap();

        let models = manager.discover_models().await.unwrap();

        assert!(matches!(models[0].status, ModelStatus::Corrupted { .. }));
        assert!(!manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn delete_removes_the_artifact_and_reports_missing_afterwards() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        std::fs::write(artifact_path(dir.path()), payload).unwrap();
        manager.discover_models().await.unwrap();

        let message = manager.delete_model("test-ultra-q8_0").await.unwrap();

        assert!(message.contains("test-ultra-q8_0"), "message: {message}");
        assert!(!artifact_path(dir.path()).exists());
        let models = manager.discover_models().await.unwrap();
        assert!(matches!(models[0].status, ModelStatus::Missing));
    }

    #[tokio::test]
    async fn delete_of_an_absent_model_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);

        manager.delete_model("test-ultra-q8_0").await.unwrap();
        assert!(matches!(
            manager.discover_models().await.unwrap()[0].status,
            ModelStatus::Missing
        ));
    }

    #[tokio::test]
    async fn delete_rejects_an_unknown_model() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);

        let error = manager.delete_model("not-a-model").await.unwrap_err();
        assert!(error.to_string().contains("Unknown transcribe.cpp model"));
    }

    #[tokio::test]
    async fn download_reports_progress_and_publishes_available() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(4096);
        let manager = manager(dir.path(), payload);
        let (url, server) = serve_body(payload).await;

        let reported = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reported_by_callback = Arc::clone(&reported);
        let sha = sha256_hex(payload);
        manager
            .download_artifact(
                "test-ultra-q8_0",
                "test-ultra-q8_0.gguf",
                &url,
                payload.len() as u64,
                &sha,
                Some(Box::new(move |progress| {
                    reported_by_callback.lock().unwrap().push(progress)
                })),
            )
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(std::fs::read(artifact_path(dir.path())).unwrap(), payload);
        let reported = reported.lock().unwrap();
        assert_eq!(reported.first(), Some(&0));
        assert_eq!(reported.last(), Some(&100));
        assert!(reported.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn download_rejects_a_truncated_artifact_and_cleans_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(4096);
        let manager = manager(dir.path(), payload);
        let (url, server) = serve_body(&payload[..64]).await;

        let error = download_from(
            &manager,
            &url,
            payload.len() as u64,
            &sha256_hex(payload),
        )
        .await
        .unwrap_err();
        server.await.unwrap();

        assert!(
            error.to_string().contains("too small") || error.to_string().contains("exactly"),
            "error: {error}"
        );
        assert!(!artifact_path(dir.path()).exists());
        assert!(matches!(
            manager.discover_models().await.unwrap()[0].status,
            ModelStatus::Missing
        ));
    }

    #[tokio::test]
    async fn download_rejects_a_well_sized_file_without_a_gguf_header() {
        let dir = tempfile::tempdir().unwrap();
        let expected = test_payload_static(1024);
        let manager = manager(dir.path(), expected);
        let bogus: &'static [u8] = Box::leak(vec![b'x'; 1024].into_boxed_slice());
        let (url, server) = serve_body(bogus).await;

        let error = download_from(
            &manager,
            &url,
            expected.len() as u64,
            &sha256_hex(expected),
        )
        .await
        .unwrap_err();
        server.await.unwrap();

        assert!(
            error
                .to_string()
                .contains("missing GGML/GGUF magic number"),
            "error: {error}"
        );
        assert!(!artifact_path(dir.path()).exists());
    }

    #[tokio::test]
    async fn download_rejects_a_correctly_sized_file_with_the_wrong_digest() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        let mut tampered = test_payload(1024);
        tampered[4] = 0xCD;
        let tampered: &'static [u8] = Box::leak(tampered.into_boxed_slice());
        let (url, server) = serve_body(tampered).await;

        let error = download_from(
            &manager,
            &url,
            payload.len() as u64,
            &sha256_hex(payload),
        )
        .await
        .unwrap_err();
        server.await.unwrap();

        assert!(error.to_string().contains("SHA-256"), "error: {error}");
        assert!(!artifact_path(dir.path()).exists());
        assert!(matches!(
            manager.discover_models().await.unwrap()[0].status,
            ModelStatus::Missing
        ));
    }

    #[tokio::test]
    async fn download_rejects_a_filename_that_escapes_the_models_directory() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        let sha = sha256_hex(payload);

        for filename in ["", "../escape.gguf", "nested/model.gguf", "..\\escape.gguf"] {
            let error = manager
                .download_artifact(
                    "test-ultra-q8_0",
                    filename,
                    "http://127.0.0.1:9/model.gguf",
                    payload.len() as u64,
                    &sha,
                    None,
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("Invalid model filename"),
                "filename {filename:?} produced: {error}"
            );
        }
    }

    #[tokio::test]
    async fn duplicate_download_is_rejected_while_one_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(4096);
        let manager = manager(dir.path(), payload);
        let sha = sha256_hex(payload);

        let _owner = manager
            .reserve_active_download("test-ultra-q8_0")
            .await
            .unwrap();

        let error = manager
            .download_artifact(
                "test-ultra-q8_0",
                "test-ultra-q8_0.gguf",
                "http://127.0.0.1:9/model.gguf",
                payload.len() as u64,
                &sha,
                None,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("already in progress"));
    }

    #[tokio::test]
    async fn cancelling_an_active_download_removes_the_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(4096);
        let manager = Arc::new(manager(dir.path(), payload));
        let (url, headers_ready, release_body, server) = serve_stalled().await;

        let download_manager = Arc::clone(&manager);
        let sha = sha256_hex(payload);
        let download = tokio::spawn(async move {
            download_manager
                .download_artifact(
                    "test-ultra-q8_0",
                    "test-ultra-q8_0.gguf",
                    &url,
                    payload.len() as u64,
                    &sha,
                    None,
                )
                .await
        });

        headers_ready.await.unwrap();
        wait_for_file(&artifact_path(dir.path())).await;

        let outcome = manager.cancel_download("test-ultra-q8_0").await.unwrap();
        assert_eq!(outcome, CancelDownloadOutcome::Cancelled);

        let error = download.await.unwrap().unwrap_err();
        assert!(is_download_cancelled(&error));
        assert!(!artifact_path(dir.path()).exists());
        assert!(matches!(
            manager.discover_models().await.unwrap()[0].status,
            ModelStatus::Missing
        ));

        let _ = release_body.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn a_cancelled_model_can_be_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(4096);
        let manager = Arc::new(manager(dir.path(), payload));
        let (stalled_url, headers_ready, release_body, stalled_server) = serve_stalled().await;

        let download_manager = Arc::clone(&manager);
        let sha = sha256_hex(payload);
        let download = tokio::spawn(async move {
            download_manager
                .download_artifact(
                    "test-ultra-q8_0",
                    "test-ultra-q8_0.gguf",
                    &stalled_url,
                    payload.len() as u64,
                    &sha,
                    None,
                )
                .await
        });
        headers_ready.await.unwrap();

        manager.cancel_download("test-ultra-q8_0").await.unwrap();
        assert!(is_download_cancelled(&download.await.unwrap().unwrap_err()));
        let _ = release_body.send(());
        let _ = stalled_server.await;

        // Retry from a clean state must succeed and end Available.
        let (url, server) = serve_body(payload).await;
        let retry_sha = sha256_hex(payload);
        manager
            .download_artifact(
                "test-ultra-q8_0",
                "test-ultra-q8_0.gguf",
                &url,
                payload.len() as u64,
                &retry_sha,
                None,
            )
            .await
            .unwrap();
        server.await.unwrap();

        assert!(manager.is_model_available("test-ultra-q8_0").await);
    }

    #[tokio::test]
    async fn cancelling_when_no_download_is_active_reports_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);

        assert_eq!(
            manager.cancel_download("test-ultra-q8_0").await.unwrap(),
            CancelDownloadOutcome::Cancelled
        );
    }

    #[tokio::test]
    async fn delete_is_rejected_while_the_model_is_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        let manager = manager(dir.path(), payload);
        let _owner = manager
            .reserve_active_download("test-ultra-q8_0")
            .await
            .unwrap();

        let error = manager.delete_model("test-ultra-q8_0").await.unwrap_err();
        assert!(error.to_string().contains("currently downloading"));
    }

    #[tokio::test]
    async fn downloaded_state_survives_a_fresh_manager_for_the_same_directory() {
        let dir = tempfile::tempdir().unwrap();
        let payload = test_payload_static(1024);
        {
            let manager = manager(dir.path(), payload);
            let (url, server) = serve_body(payload).await;
            download_from(&manager, &url, payload.len() as u64, &sha256_hex(payload))
                .await
                .unwrap();
            server.await.unwrap();
        }

        // A restart constructs a new manager over the same app-data directory.
        let restarted = manager(dir.path(), payload);
        let models = restarted.discover_models().await.unwrap();
        assert!(matches!(models[0].status, ModelStatus::Available));
    }
}
