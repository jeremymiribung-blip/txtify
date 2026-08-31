use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::error::TxtifyError;
use crate::core::types::ConversionMode;

/// Sidecar-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    /// Python executable path. If None, auto-detect (VIRTUAL_ENV/.venv/python3).
    #[serde(default)]
    pub python_path: Option<String>,
    /// GLM backend: "auto", "transformers", "vllm", "sglang", "ollama".
    #[serde(default = "default_backend")]
    pub glm_backend: String,
    /// HuggingFace model ID for GLM-OCR.
    #[serde(default = "default_model")]
    pub glm_model: String,
}

fn default_backend() -> String {
    "auto".to_string()
}

fn default_model() -> String {
    "zai-org/GLM-OCR".to_string()
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            python_path: None,
            glm_backend: default_backend(),
            glm_model: default_model(),
        }
    }
}

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub mode: ConversionMode,
    #[serde(default)]
    pub sidecar_path: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub sidecar: SidecarConfig,
}

impl Config {
    /// load — fn for txtify.
    pub fn load() -> Result<Self, TxtifyError> {
        for path in Self::candidate_paths() {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        // Try TOML first, then JSON fallback
                        let parsed: Result<Self, _> = toml::from_str(&content);
                        match parsed {
                            Ok(cfg) => {
                                tracing::debug!(?path, "loaded config");
                                return Ok(cfg);
                            }
                            Err(e) => {
                                // Fallback to JSON
                                if let Ok(cfg) = serde_json::from_str::<Self>(&content) {
                                    tracing::debug!(?path, "loaded config as json");
                                    return Ok(cfg);
                                }
                                // Return TOML error as ConfigError
                                return Err(TxtifyError::ConfigError(format!(
                                    "failed to parse {}: {e}",
                                    path.display()
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(?path, error = %e, "failed to read config");
                        continue;
                    }
                }
            }
        }
        Ok(Self::default())
    }

    /// from_json — fn for txtify.
    pub fn from_json(json: &str) -> Result<Self, TxtifyError> {
        serde_json::from_str(json).map_err(|e| TxtifyError::ConfigError(e.to_string()))
    }

    /// to_json — fn for txtify.
    pub fn to_json(&self) -> Result<String, TxtifyError> {
        serde_json::to_string_pretty(self).map_err(|e| TxtifyError::ConfigError(e.to_string()))
    }

    /// Paths probed for config, in priority order.
    pub fn candidate_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // exe dir / txtify.toml
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("txtify.toml"));
                paths.push(dir.join("config.toml"));
            }
        }

        // cwd / txtify.toml
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join("txtify.toml"));
        }

        // platform-specific
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let base = PathBuf::from(appdata).join("txtify");
                paths.push(base.join("config.toml"));
                paths.push(base.join("txtify.toml"));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // XDG_CONFIG_HOME or ~/.config
            let config_home = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|h| PathBuf::from(h).join(".config"))
                        .unwrap_or_else(|_| PathBuf::from(".config"))
                });
            let base = config_home.join("txtify");
            paths.push(base.join("config.toml"));
            paths.push(base.join("txtify.toml"));
        }

        paths
    }

    /// Return the path that would be used (first existing) or the default platform path.
    pub fn resolved_path() -> Option<PathBuf> {
        for p in Self::candidate_paths() {
            if p.exists() {
                return Some(p);
            }
        }
        // Return preferred default (last platform path) as suggestion
        Self::candidate_paths().last().cloned()
    }
}
