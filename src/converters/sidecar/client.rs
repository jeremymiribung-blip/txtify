use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::config::SidecarConfig;
use crate::core::error::TxtifyError;

/// Request sent to sidecar via JSON stdin.
#[derive(Debug, Serialize)]
struct SidecarRequest {
    op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

/// Response from sidecar via JSON stdout.
#[derive(Debug, Deserialize)]
struct SidecarResponse {
    markdown: Option<String>,
    error: Option<String>,
    pages: Option<u32>,
    #[serde(default)]
    status: Option<String>,
}

/// Health response.
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: Option<String>,
    pub error: Option<String>,
    pub backend: Option<String>,
    pub engine: Option<String>,
}

/// Client for the GLM-OCR Python sidecar.
///
/// Spawns `python sidecar/txtify_sidecar.py` via `tokio::process::Command`,
/// detects python via `VIRTUAL_ENV` / `.venv/python3`, communicates JSON over stdin/stdout,
/// 60s timeout, health check.
#[derive(Debug, Clone)]
pub struct SidecarClient {
    python_path: String,
    sidecar_script: PathBuf,
    backend: String,
    model: String,
    timeout: Duration,
}

impl SidecarClient {
    /// Create client from config.
    pub fn from_config(config: &SidecarConfig) -> Self {
        let python_path = config.python_path.clone().unwrap_or_else(detect_python);
        let sidecar_script = find_sidecar_script();
        Self {
            python_path,
            sidecar_script,
            backend: config.glm_backend.clone(),
            model: config.glm_model.clone(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Create client with explicit paths (for testing).
    pub fn new(
        python_path: String,
        sidecar_script: PathBuf,
        backend: String,
        model: String,
    ) -> Self {
        Self {
            python_path,
            sidecar_script,
            backend,
            model,
            timeout: Duration::from_secs(60),
        }
    }

    /// Override timeout (for testing).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Check if sidecar is available (python + script exist, not checking glm import).
    /// Handles `python3` on PATH as well as absolute paths.
    pub fn is_available(&self) -> bool {
        if !self.sidecar_script.exists() {
            return false;
        }
        // If python_path contains a slash, check path existence
        if self.python_path.contains('/') {
            Path::new(&self.python_path).exists()
        } else {
            // Assume it is on PATH (e.g., "python3"); try to probe via Command existence
            // We consider it available if spawning `python3 --version` would succeed; for fast check, just return true
            // Actual spawn will validate.
            true
        }
    }

    /// Static version: checks default detected python + script.
    pub fn is_available_global() -> bool {
        let cfg = SidecarConfig::default();
        Self::from_config(&cfg).is_available()
    }

    /// Perform health check by spawning sidecar and sending `{"op":"health"}`.
    pub async fn check_health(&self) -> Result<HealthResponse, TxtifyError> {
        self.run_request(SidecarRequest {
            op: "health".to_string(),
            path: None,
            to: None,
            engine: None,
            backend: Some(self.backend.clone()),
            model: Some(self.model.clone()),
        })
        .await
        .map(|resp| HealthResponse {
            status: resp.status,
            error: resp.error,
            backend: None,
            engine: None,
        })
        .map_err(|_| TxtifyError::ConversionFailed("health check failed".to_string()))
    }

    /// More lenient health check returning bool.
    pub async fn health_ok(&self) -> bool {
        // Directly spawn and check raw health response
        match self.raw_health().await {
            Ok(json) => {
                json.get("status").and_then(|v| v.as_str()) == Some("ok")
                    || json.get("error").is_none()
            }
            Err(_) => false,
        }
    }

    async fn raw_health(&self) -> Result<serde_json::Value, TxtifyError> {
        let req = serde_json::json!({"op":"health"});
        let line =
            serde_json::to_string(&req).map_err(|e| TxtifyError::ConfigError(e.to_string()))?;
        let out = self.spawn_and_communicate(&line).await?;
        serde_json::from_str(&out).map_err(|e| {
            TxtifyError::ConversionFailed(format!("invalid health json: {e} output={out}"))
        })
    }

    /// Convert a file via sidecar.
    pub async fn convert(
        &self,
        path: &Path,
        to: &str,
        engine: &str,
    ) -> Result<(String, Option<u32>), TxtifyError> {
        if !self.is_available() {
            return Err(TxtifyError::SidecarNotFound(format!(
                "sidecar not found (python={}, script={}). Hint: pip install -r sidecar/requirements.txt",
                self.python_path,
                self.sidecar_script.display()
            )));
        }

        let req = SidecarRequest {
            op: "convert".to_string(),
            path: Some(path.display().to_string()),
            to: Some(to.to_string()),
            engine: Some(engine.to_string()),
            backend: Some(self.backend.clone()),
            model: Some(self.model.clone()),
        };

        let resp = self.run_request(req).await?;

        if let Some(err) = resp.error {
            return Err(TxtifyError::ConversionFailed(err));
        }

        let markdown = resp.markdown.unwrap_or_default();
        Ok((markdown, resp.pages))
    }

    async fn run_request(&self, req: SidecarRequest) -> Result<SidecarResponse, TxtifyError> {
        let line =
            serde_json::to_string(&req).map_err(|e| TxtifyError::ConfigError(e.to_string()))?;
        let out = self.spawn_and_communicate(&line).await?;
        let resp: SidecarResponse = serde_json::from_str(&out).map_err(|e| {
            TxtifyError::ConversionFailed(format!("invalid sidecar json: {e} output={out}"))
        })?;
        Ok(resp)
    }

    async fn spawn_and_communicate(&self, request_line: &str) -> Result<String, TxtifyError> {
        if self.python_path.contains('/') && !Path::new(&self.python_path).exists() {
            return Err(TxtifyError::SidecarNotFound(format!(
                "python not found at {} (hint: pip install -r sidecar/requirements.txt, or set sidecar.python_path in config)",
                self.python_path
            )));
        }
        if !self.sidecar_script.exists() {
            return Err(TxtifyError::SidecarNotFound(format!(
                "sidecar script not found at {} (hint: ensure sidecar/txtify_sidecar.py exists)",
                self.sidecar_script.display()
            )));
        }

        let mut child = Command::new(&self.python_path)
            .arg(&self.sidecar_script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                TxtifyError::SidecarNotFound(format!(
                    "failed to spawn sidecar {} {}: {e}. Hint: pip install -r sidecar/requirements.txt",
                    self.python_path,
                    self.sidecar_script.display()
                ))
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            TxtifyError::ConversionFailed("failed to open sidecar stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            TxtifyError::ConversionFailed("failed to open sidecar stdout".to_string())
        })?;

        // Write request with timeout
        let req_line = format!("{request_line}\n");
        let write_fut = async {
            stdin.write_all(req_line.as_bytes()).await.map_err(|e| {
                TxtifyError::ConversionFailed(format!("failed to write to sidecar stdin: {e}"))
            })?;
            stdin.flush().await.map_err(|e| {
                TxtifyError::ConversionFailed(format!("failed to flush sidecar stdin: {e}"))
            })?;
            // Close stdin to signal EOF (drop)
            drop(stdin);
            Ok::<(), TxtifyError>(())
        };

        tokio::time::timeout(self.timeout, write_fut)
            .await
            .map_err(|_| TxtifyError::Timeout)??;

        // Read response line with timeout
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        let read_fut = reader.read_line(&mut line);
        let n = tokio::time::timeout(self.timeout, read_fut)
            .await
            .map_err(|_| {
                // Kill child on timeout
                let _ = child.start_kill();
                TxtifyError::Timeout
            })?
            .map_err(|e| {
                TxtifyError::ConversionFailed(format!("failed to read sidecar stdout: {e}"))
            })?;

        // Wait for child to exit (with timeout)
        let status_fut = child.wait();
        let status = tokio::time::timeout(Duration::from_secs(5), status_fut)
            .await
            .map_err(|_| TxtifyError::Timeout)?
            .map_err(|e| TxtifyError::ConversionFailed(format!("sidecar wait failed: {e}")))?;

        if n == 0 {
            // No output; try to get stderr if possible (child already waited)
            return Err(TxtifyError::ConversionFailed(format!(
                "sidecar produced no output (exit={}). Hint: pip install -r sidecar/requirements.txt and ensure glm-ocr installed",
                status
            )));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err(TxtifyError::ConversionFailed(format!(
                "sidecar returned empty line (exit={})",
                status
            )));
        }

        Ok(trimmed.to_string())
    }
}

/// Detect python executable.
///
/// Priority:
/// 1. VIRTUAL_ENV env var -> $VIRTUAL_ENV/bin/python3
/// 2. .venv/bin/python3 relative to CARGO_MANIFEST_DIR or current dir
/// 3. `python3` fallback
pub fn detect_python() -> String {
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let p = PathBuf::from(venv).join("bin").join("python3");
        if p.exists() {
            return p.display().to_string();
        }
        let p2 = PathBuf::from(std::env::var("VIRTUAL_ENV").unwrap_or_default())
            .join("bin")
            .join("python");
        if p2.exists() {
            return p2.display().to_string();
        }
    }

    // Check .venv in manifest dir or cwd
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".venv")
            .join("bin")
            .join("python3"),
        PathBuf::from(".venv").join("bin").join("python3"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".venv")
            .join("bin")
            .join("python"),
        PathBuf::from(".venv").join("bin").join("python"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.display().to_string();
        }
    }

    // Fallback to python3 on PATH
    "python3".to_string()
}

/// Find sidecar script path.
pub fn find_sidecar_script() -> PathBuf {
    // Try CARGO_MANIFEST_DIR/sidecar/txtify_sidecar.py first
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecar")
        .join("txtify_sidecar.py");
    if manifest.exists() {
        return manifest;
    }
    // Fallback to sidecar/txtify_sidecar.py relative to cwd
    let cwd = PathBuf::from("sidecar").join("txtify_sidecar.py");
    if cwd.exists() {
        return cwd;
    }
    // Return manifest path even if missing (for error message)
    manifest
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn detect_python_returns_nonempty() {
        let p = detect_python();
        assert!(!p.is_empty());
    }

    #[test]
    fn find_sidecar_script_returns_path() {
        let p = find_sidecar_script();
        // Should end with txtify_sidecar.py
        assert!(p.to_string_lossy().contains("txtify_sidecar.py"));
    }

    #[tokio::test]
    async fn is_available_false_when_missing() {
        let client = SidecarClient::new(
            "/nonexistent/python3".to_string(),
            PathBuf::from("/nonexistent/txtify_sidecar.py"),
            "auto".to_string(),
            "zai-org/GLM-OCR".to_string(),
        );
        assert!(!client.is_available());
        let err = client
            .convert(Path::new("/tmp/a.pdf"), "md", "glm_ocr")
            .await
            .unwrap_err();
        assert!(matches!(err, TxtifyError::SidecarNotFound(_)));
        let msg = err.to_string();
        assert!(
            msg.contains("pip install -r sidecar/requirements.txt"),
            "hint missing: {msg}"
        );
    }

    #[tokio::test]
    async fn convert_missing_python_returns_sidecar_not_found() {
        let client = SidecarClient::new(
            "/tmp/nonexistent_python_xyz".to_string(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("sidecar")
                .join("txtify_sidecar.py"),
            "auto".to_string(),
            "zai-org/GLM-OCR".to_string(),
        );
        let res = client
            .convert(Path::new("/tmp/a.pdf"), "md", "glm_ocr")
            .await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, TxtifyError::SidecarNotFound(_)));
    }
}
