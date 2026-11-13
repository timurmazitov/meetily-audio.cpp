//! Engine-agnostic model download mechanism.
//!
//! Extracted from the Whisper engine downloader so that every transcription
//! engine (Whisper today, audio.cpp tomorrow) shares the same download
//! mechanics: HTTP streaming with progress reporting, cooperative
//! cancellation, and validation of the downloaded file (magic bytes and
//! minimum size). Engine-specific concerns — model catalogs, status
//! bookkeeping, reservation lifecycles — stay in the engines themselves.

use std::future::Future;
use std::path::Path;

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

/// Error returned when a download is interrupted by user cancellation.
#[derive(Debug, thiserror::Error)]
#[error("Download cancelled by user")]
pub struct DownloadCancelled;

/// Returns true when the error chain represents a user-cancelled download.
pub fn is_download_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<DownloadCancelled>().is_some()
}

/// Validate that a file looks like a model by checking its header for a
/// known GGML/GGUF magic number.
pub async fn validate_model_magic(model_path: &Path) -> Result<()> {
    let mut file = fs::File::open(model_path)
        .await
        .map_err(|e| anyhow!("Failed to open model file: {}", e))?;

    // Read the first 8 bytes to check for GGML/GGUF magic numbers across
    // versions and endianness.
    let mut buffer = [0u8; 8];
    file.read_exact(&mut buffer)
        .await
        .map_err(|e| anyhow!("Failed to read model file header: {}", e))?;

    if buffer.starts_with(b"ggml")
        || buffer.starts_with(b"GGUF")
        || buffer.starts_with(b"ggmf")
        || buffer.starts_with(b"lmgg")
        || buffer.starts_with(b"FUGU")
        || buffer.starts_with(b"fmgg")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "Invalid model file: missing GGML/GGUF magic number. Found: {:?}",
            String::from_utf8_lossy(&buffer[..4])
        ))
    }
}

/// Validate a freshly downloaded model file: the header must carry a known
/// GGML/GGUF magic number and the file must be at least `expected_min_size_bytes`.
pub async fn validate_downloaded_model(
    model_path: &Path,
    expected_min_size_bytes: u64,
) -> Result<()> {
    validate_model_magic(model_path).await?;

    let metadata = fs::metadata(model_path)
        .await
        .map_err(|e| anyhow!("Failed to read downloaded model file metadata: {}", e))?;

    if metadata.len() < expected_min_size_bytes {
        return Err(anyhow!(
            "Downloaded model file is too small: {} bytes (expected at least {} bytes)",
            metadata.len(),
            expected_min_size_bytes
        ));
    }

    Ok(())
}

/// Download `url` into `file_path` with cooperative cancellation and
/// throttled progress reporting.
///
/// The progress hook is invoked with `0` before the first chunk, with
/// intermediate percentages (at most once per percent, or at least every two
/// seconds), and with `100` after the response body has been fully written
/// and flushed. Partial-file cleanup on failure or cancellation is the
/// caller's responsibility.
pub async fn download_file<F, Fut>(
    url: &str,
    file_path: &Path,
    cancellation: &CancellationToken,
    progress: F,
) -> Result<()>
where
    F: Fn(u8) -> Fut + Send,
    Fut: Future<Output = ()> + Send,
{
    if cancellation.is_cancelled() {
        return Err(DownloadCancelled.into());
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("Meetily/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| anyhow!("Failed to create download client: {}", e))?;

    let response = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(DownloadCancelled.into()),
        response = client.get(url).send() => response
            .map_err(|e| anyhow!("Failed to start download: {}", e))?,
    };

    if !response.status().is_success() {
        return Err(anyhow!("Download failed with status: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut file = fs::File::create(file_path)
        .await
        .map_err(|e| anyhow!("Failed to create file: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;
    let mut last_progress_report = 0u8;
    let mut last_report_time = std::time::Instant::now();

    progress(0).await;

    loop {
        let chunk_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(DownloadCancelled.into()),
            chunk_result = stream.next() => chunk_result,
        };

        let Some(chunk_result) = chunk_result else {
            break;
        };

        if cancellation.is_cancelled() {
            return Err(DownloadCancelled.into());
        }

        let chunk = chunk_result.map_err(|e| anyhow!("Failed to read chunk: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| anyhow!("Failed to write chunk to file: {}", e))?;

        downloaded += chunk.len() as u64;
        let reported = if total_size > 0 {
            ((downloaded as f64 / total_size as f64) * 100.0) as u8
        } else {
            0
        };

        let time_since_last_report = last_report_time.elapsed().as_secs();
        if reported >= last_progress_report + 1 || reported == 100 || time_since_last_report >= 2 {
            progress(reported).await;
            last_progress_report = reported;
            last_report_time = std::time::Instant::now();
        }
    }

    progress(100).await;

    file.flush()
        .await
        .map_err(|e| anyhow!("Failed to flush file: {}", e))?;

    if cancellation.is_cancelled() {
        return Err(DownloadCancelled.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    async fn response_http_server(
        content_length: usize,
        body: &'static [u8],
    ) -> (String, oneshot::Receiver<Vec<u8>>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_sent, request_received) = oneshot::channel();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let bytes_read = socket.read(&mut buffer).await.unwrap();
                if bytes_read == 0 {
                    break;
                }

                request.extend_from_slice(&buffer[..bytes_read]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            request_sent.send(request).unwrap();

            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(headers.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.flush().await.unwrap();
        });

        (format!("http://{address}/model.bin"), request_received, server)
    }

    async fn stalled_http_server(
    ) -> (
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
            socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            socket.flush().await.unwrap();
            headers_sent.send(()).unwrap();
            let _ = body_released.await;
            let _ = socket.write_all(b"test").await;
        });

        (format!("http://{address}/model.bin"), headers_ready, release_body, server)
    }

    #[tokio::test]
    async fn validate_model_magic_accepts_known_model_headers() {
        let dir = tempfile::tempdir().unwrap();
        for (index, magic) in [b"ggml", b"GGUF", b"ggmf", b"lmgg", b"FUGU", b"fmgg"]
            .iter()
            .enumerate()
        {
            let path = dir.path().join(format!("model-{index}.bin"));
            let mut content = magic.to_vec();
            content.extend_from_slice(b"\0\0\0\0");
            std::fs::write(&path, content).unwrap();

            validate_model_magic(&path)
                .await
                .unwrap_or_else(|error| panic!("magic {:?} should be accepted: {error}", magic));
        }
    }

    #[tokio::test]
    async fn validate_model_magic_rejects_non_model_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-model.bin");
        std::fs::write(&path, b"not-a-model").unwrap();

        let error = validate_model_magic(&path).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("Invalid model file: missing GGML/GGUF magic number"));
    }

    #[tokio::test]
    async fn validate_downloaded_model_rejects_undersized_file_with_valid_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggml-tiny.bin");
        std::fs::write(&path, b"ggml\0\0\0\0").unwrap();

        let error = validate_downloaded_model(&path, 1024).await.unwrap_err();
        assert!(error.to_string().contains("too small"));
    }

    #[tokio::test]
    async fn validate_downloaded_model_accepts_file_at_expected_min_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggml-tiny.bin");
        let mut content = b"ggml".to_vec();
        content.extend_from_slice(&vec![0_u8; 1024]);
        std::fs::write(&path, content).unwrap();

        validate_downloaded_model(&path, 1024).await.unwrap();
    }

    #[tokio::test]
    async fn download_file_writes_body_and_reports_progress_from_zero_to_hundred() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let body = b"ggml\0\0\0\0";
        let (url, _request, server) = response_http_server(body.len(), body).await;

        let reported = Arc::new(Mutex::new(Vec::new()));
        let reported_by_hook = Arc::clone(&reported);
        download_file(&url, &file_path, &CancellationToken::new(), move |progress| {
            let reported = Arc::clone(&reported_by_hook);
            async move {
                reported.lock().unwrap().push(progress);
            }
        })
        .await
        .unwrap();
        server.await.unwrap();

        assert_eq!(std::fs::read(&file_path).unwrap(), body);
        let reported = reported.lock().unwrap();
        assert_eq!(reported.first(), Some(&0));
        assert_eq!(reported.last(), Some(&100));
        assert!(
            reported.windows(2).all(|pair| pair[0] <= pair[1]),
            "progress must never go backwards: {reported:?}"
        );
    }

    #[tokio::test]
    async fn download_file_sends_meetily_user_agent() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let (url, request, server) = response_http_server(4, b"ggml").await;

        download_file(&url, &file_path, &CancellationToken::new(), |_| async {})
            .await
            .unwrap();
        let request = String::from_utf8(request.await.unwrap()).unwrap();
        server.await.unwrap();

        assert!(request.to_ascii_lowercase().contains(&format!(
            "user-agent: meetily/{}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    #[tokio::test]
    async fn download_file_rejects_http_error_status() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });

        let error = download_file(
            &format!("http://{address}/model.bin"),
            &file_path,
            &CancellationToken::new(),
            |_| async {},
        )
        .await
        .unwrap_err();
        server.await.unwrap();

        assert!(error.to_string().contains("Download failed with status: 500"));
    }

    #[tokio::test]
    async fn download_file_fails_when_response_body_is_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let (url, _request, server) = response_http_server(8, b"ggml").await;

        let error = download_file(&url, &file_path, &CancellationToken::new(), |_| async {})
            .await
            .unwrap_err();
        server.await.unwrap();

        assert!(error.to_string().contains("Failed to read chunk"));
    }

    #[tokio::test]
    async fn download_file_reports_cancellation_when_token_is_already_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let error = download_file("http://127.0.0.1:9/model.bin", &file_path, &cancellation, |_| async {})
            .await
            .unwrap_err();

        assert!(is_download_cancelled(&error));
    }

    #[tokio::test]
    async fn download_file_cancellation_interrupts_a_stalled_download() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("model.bin");
        let (url, headers_ready, release_body, server) = stalled_http_server().await;
        let cancellation = CancellationToken::new();

        let download_cancellation = cancellation.clone();
        let download = tokio::spawn(async move {
            download_file(&url, &file_path, &download_cancellation, |_| async {}).await
        });

        headers_ready.await.unwrap();
        cancellation.cancel();

        let error = download.await.unwrap().unwrap_err();
        assert!(is_download_cancelled(&error));

        let _ = release_body.send(());
        let _ = server.await;
    }
}
