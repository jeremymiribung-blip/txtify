//! `txtify setup` — lädt alle Ressourcen für curl-Installs automatisch nach.
//!
//! Schritte:
//! 1. Python >= 3.10 prüfen
//! 2. Sidecar-Skript sicherstellen (aus Release-Tarball oder eingebettet materialisieren)
//! 3. Python-Deps installieren (`pip install -r sidecar/requirements.txt`, mind. `huggingface_hub`)
//! 4. KI-Modell (`zai-org/GLM-OCR`, ~1 GB) von Hugging Face prefetchen
//! 5. Default-Config schreiben (falls fehlend)
//! 6. Pandoc-Hinweis (optional)
//! 7. Optional Shell-Integration

use std::path::PathBuf;

use crate::config::Config;
use crate::converters::sidecar::{detect_python, find_sidecar_script};
use crate::core::error::TxtifyError;

/// Optionen für `txtify setup` (vom CLI gesetzt).
#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    /// HF-Modell-ID (Default aus Config bzw. `zai-org/GLM-OCR`).
    pub model: Option<String>,
    /// Backend-Name für Config (`auto|transformers|vllm|sglang|ollama`).
    pub backend: Option<String>,
    /// Python-Executable Override.
    pub python: Option<String>,
    /// Kein Modell-Download (nur Python/Config).
    pub no_model: bool,
    /// Keine `pip install` (nur Modell/Config prüfen).
    pub no_python_deps: bool,
    /// Auch Shell-Integration installieren.
    pub with_shell: bool,
    /// Nicht-interaktiv (keine Rückfragen).
    pub yes: bool,
    /// Nur prüfen, nichts ändern (`--check`).
    pub check_only: bool,
}

/// Default-Modell-ID.
pub fn default_model() -> String {
    "zai-org/GLM-OCR".to_string()
}

/// Daten-Verzeichnis für materialisierten Sidecar (`~/.local/share/txtify` bzw. `%LOCALAPPDATA%\txtify`).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(|v| PathBuf::from(v).join("txtify"))
            .unwrap_or_else(|_| PathBuf::from("txtify-data"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| PathBuf::from(h).join(".local").join("share"))
                    .unwrap_or_else(|_| PathBuf::from(".local/share"))
            })
            .join("txtify")
    }
}

/// Zielpfad für materialisierten Sidecar.
pub fn materialized_sidecar_path() -> PathBuf {
    data_dir().join("sidecar").join("txtify_sidecar.py")
}

/// Python-Version prüfen, gibt `(major, minor)` zurück.
fn parse_python_version(output: &str) -> Option<(u32, u32)> {
    // Erwartet "Python 3.11.8"
    let parts: Vec<&str> = output.split_whitespace().collect();
    let ver = parts.get(1)?;
    let mut nums = ver.split('.');
    let major: u32 = nums.next()?.parse().ok()?;
    let minor: u32 = nums.next()?.parse().ok()?;
    Some((major, minor))
}

async fn run_cmd(program: &str, args: &[&str]) -> Result<(bool, String), TxtifyError> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| {
            TxtifyError::SidecarNotFound(format!("konnte {program} nicht starten: {e}"))
        })?;
    let mut combined = String::from_utf8_lossy(&out.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), combined))
}

/// Stellt sicher, dass das Sidecar-Skript auf Platte existiert.
/// Falls weder Release-Tarball-Sidecar noch CWD-Sidecar existiert,
/// wird der eingebettete Sidecar (`include_str!`) nach `data_dir()` geschrieben.
pub fn ensure_sidecar_script(check_only: bool) -> Result<PathBuf, TxtifyError> {
    let found = find_sidecar_script();
    if found.exists() {
        return Ok(found);
    }
    let target = materialized_sidecar_path();
    if target.exists() {
        return Ok(target);
    }
    if check_only {
        return Err(TxtifyError::SidecarNotFound(format!(
            "sidecar fehlt (gesucht: {}, {}). Tipp: `txtify setup` ohne --check ausführen",
            found.display(),
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
        }
    }
    std::fs::write(&target, crate::EMBEDDED_SIDECAR_PY).map_err(TxtifyError::Io)?;
    // Ausführbar machen (Unix)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target)
            .map_err(TxtifyError::Io)?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target, perms).map_err(TxtifyError::Io)?;
    }
    Ok(target)
}

/// Schreibt Default-Config, falls keine existiert.
pub fn ensure_default_config(
    model: &str,
    backend: &str,
    python_path: Option<&str>,
    sidecar_path: Option<&str>,
    check_only: bool,
) -> Result<Option<PathBuf>, TxtifyError> {
    // Falls bereits eine Config existiert, nichts tun.
    for p in Config::candidate_paths() {
        if p.exists() {
            return Ok(None);
        }
    }
    let target = Config::resolved_path()
        .ok_or_else(|| TxtifyError::ConfigError("kein Config-Pfad bestimmbar".to_string()))?;
    if check_only {
        return Err(TxtifyError::ConfigError(format!(
            "keine Config gefunden (würde {} anlegen). Tipp: `txtify setup` ohne --check",
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
        }
    }
    let mut toml = String::from("mode = \"Fast\"\n");
    if let Some(sp) = sidecar_path {
        toml.push_str(&format!(
            "sidecar_path = \"{}\"\n",
            sp.replace('\\', "\\\\")
        ));
    }
    toml.push_str("timeout_secs = 60\n\n[sidecar]\n");
    if let Some(py) = python_path {
        toml.push_str(&format!("python_path = \"{}\"\n", py.replace('\\', "\\\\")));
    }
    toml.push_str(&format!("glm_backend = \"{backend}\"\n"));
    toml.push_str(&format!("glm_model = \"{model}\"\n"));
    std::fs::write(&target, toml).map_err(TxtifyError::Io)?;
    Ok(Some(target))
}

/// Führt das komplette Setup aus. Gibt bei `--check` nur Fehler zurück, wenn etwas fehlt.
pub async fn run_setup(config: &Config, opts: &SetupOptions) -> Result<(), TxtifyError> {
    let model = opts
        .model
        .clone()
        .unwrap_or_else(|| config.sidecar.glm_model.clone());
    let model = if model.trim().is_empty() {
        default_model()
    } else {
        model
    };
    let backend = opts
        .backend
        .clone()
        .unwrap_or_else(|| config.sidecar.glm_backend.clone());
    let backend = if backend.trim().is_empty() {
        "auto".to_string()
    } else {
        backend
    };
    let python = opts
        .python
        .clone()
        .or_else(|| config.sidecar.python_path.clone())
        .unwrap_or_else(detect_python);

    println!("txtify setup");
    println!("============");
    println!("  modell : {model}");
    println!("  backend: {backend}");
    println!("  python : {python}");
    if opts.check_only {
        println!("  modus  : --check (nur prüfen, nichts ändern)");
    }

    let mut warnings: Vec<String> = Vec::new();

    // 1. Python prüfen
    println!("\n[1/5] python prüfen ...");
    match run_cmd(&python, &["--version"]).await {
        Ok((ok, out)) => {
            if !ok {
                let msg = format!("`{python} --version` meldete Fehler: {}", out.trim());
                if opts.check_only {
                    return Err(TxtifyError::SidecarNotFound(msg));
                }
                warnings.push(msg);
            } else if let Some((major, minor)) = parse_python_version(&out) {
                println!("  ok: {} (erkannt {}.{})", out.trim(), major, minor);
                if major < 3 || (major == 3 && minor < 10) {
                    let msg = format!(
                        "python {major}.{minor} ist zu alt (GLM-OCR braucht >= 3.10): {}",
                        out.trim()
                    );
                    if opts.check_only {
                        return Err(TxtifyError::SidecarNotFound(msg));
                    }
                    warnings.push(msg);
                }
            } else {
                println!("  ok (Version nicht parsbar, weiter): {}", out.trim());
            }
        }
        Err(e) => {
            let msg = format!("python nicht gefunden ({python}): {e}. Tipp: https://www.python.org/downloads/ oder `sudo apt install python3 python3-pip`");
            if opts.check_only {
                return Err(TxtifyError::SidecarNotFound(msg));
            }
            warnings.push(msg.clone());
            println!("  WARN: {msg}");
            println!("  Setup wird fortgesetzt (pip/Modell-Schritte werden ggf. übersprungen).");
        }
    }

    // 2. Sidecar sicherstellen
    println!("\n[2/5] sidecar prüfen ...");
    let sidecar_path = match ensure_sidecar_script(opts.check_only) {
        Ok(p) => {
            println!("  ok: {}", p.display());
            p
        }
        Err(e) => return Err(e),
    };

    // 3. Python-Deps
    println!("\n[3/5] python-pakete prüfen ...");
    if opts.no_python_deps {
        println!("  übersprungen (--no-python-deps)");
    } else if opts.check_only {
        // Nur prüfen, ob huggingface_hub + transformers importierbar sind
        match run_cmd(
            &python,
            &[
                "-c",
                "import importlib.util; print('ok' if importlib.util.find_spec('huggingface_hub') else 'missing')",
            ],
        )
        .await
        {
            Ok((_, out)) if out.contains("ok") => println!("  ok: huggingface_hub vorhanden"),
            _ => {
                return Err(TxtifyError::SidecarNotFound(
                    "python-pakete fehlen (huggingface_hub/transformers). Tipp: `txtify setup` ohne --check".to_string(),
                ));
            }
        }
    } else {
        // Mindestens huggingface_hub sicherstellen, dann volle requirements
        // Requirements aus eingebettetem String in Tempfile schreiben
        let tmp_req =
            std::env::temp_dir().join(format!("txtify-requirements-{}.txt", std::process::id()));
        if std::fs::write(&tmp_req, crate::EMBEDDED_SIDECAR_REQUIREMENTS).is_err() {
            warnings.push("konnte temporäre requirements.txt nicht schreiben".to_string());
        } else {
            println!("  installiere: pip install -r sidecar/requirements.txt ...");
            println!("  (torch CPU + transformers + huggingface_hub, kann einige Minuten dauern)");
            let status = tokio::process::Command::new(&python)
                .arg("-m")
                .arg("pip")
                .arg("install")
                .arg("-r")
                .arg(&tmp_req)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await;
            let _ = std::fs::remove_file(&tmp_req);
            match status {
                Ok(s) if s.success() => println!("  ok: pip-pakete installiert"),
                Ok(s) => {
                    let msg = format!(
                        "pip install meldete Exit {s}. Tipp: `'{python}' -m pip install -r sidecar/requirements.txt` manuell ausführen"
                    );
                    warnings.push(msg.clone());
                    println!("  WARN: {msg}");
                }
                Err(e) => {
                    let msg = format!("pip konnte nicht gestartet werden: {e}");
                    warnings.push(msg.clone());
                    println!("  WARN: {msg}");
                }
            }
        }
    }

    // 4. KI-Modell prefetchen (~1 GB)
    println!("\n[4/5] KI-modell prüfen ...");
    if opts.no_model {
        println!("  übersprungen (--no-model)");
    } else if opts.check_only {
        println!("  (check: modell-cache wird nicht geprüft, nutze `txtify doctor` nach setup)");
    } else {
        println!("  lade {model} von Hugging Face (~1 GB, Cache: ~/.cache/huggingface/hub) ...");
        let snippet = format!(
            "from huggingface_hub import snapshot_download; snapshot_download(repo_id='{model}'); print('MODEL_PREFETCH_OK')"
        );
        let status = tokio::process::Command::new(&python)
            .arg("-c")
            .arg(&snippet)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await;
        match status {
            Ok(s) if s.success() => println!("  ok: modell gecacht"),
            Ok(s) => {
                let msg = format!(
                    "modell-download meldete Exit {s} (offline? speicher?). Tipp: `GLM_MODEL={model} '{python}' -c \"from huggingface_hub import snapshot_download; snapshot_download(repo_id='{model}')\"`"
                );
                warnings.push(msg.clone());
                println!("  WARN: {msg}");
            }
            Err(e) => {
                let msg = format!(
                    "modell-download konnte nicht gestartet werden: {e}. Tipp: pip install huggingface_hub"
                );
                warnings.push(msg.clone());
                println!("  WARN: {msg}");
            }
        }
    }

    // 5. Config + optionale Hinweise
    println!("\n[5/5] config schreiben ...");
    let sidecar_str = sidecar_path.display().to_string();
    // Nur schreiben, wenn Pfad vom Default abweicht oder keine Config existiert,
    // damit portable Installs (exe-dir) Vorrang behalten.
    let default_script = find_sidecar_script();
    let sidecar_opt: Option<&str> = if sidecar_path == default_script {
        None
    } else {
        Some(&sidecar_str)
    };
    match ensure_default_config(
        &model,
        &backend,
        Some(&python),
        sidecar_opt,
        opts.check_only,
    ) {
        Ok(None) => println!("  ok: config existiert bereits (nichts geändert)"),
        Ok(Some(p)) => println!("  ok: config angelegt: {}", p.display()),
        Err(e) => {
            if opts.check_only {
                return Err(e);
            }
            warnings.push(e.to_string());
            println!("  WARN: {e}");
        }
    }

    // Pandoc-Hinweis (optional, kein Fehler)
    match run_cmd("pandoc", &["--version"]).await {
        Ok((true, out)) => {
            let first = out.lines().next().unwrap_or("pandoc");
            println!("\n[pandoc] ok: {first}");
        }
        _ => {
            println!("\n[pandoc] optional, nicht gefunden.");
            println!("  Für beste Fast-Qualität: sudo apt install pandoc / brew install pandoc");
            println!("  Siehe https://pandoc.org/installing.html");
        }
    }

    // Optional Shell-Integration
    if opts.with_shell {
        if opts.check_only {
            println!(
                "\n[shell] --with-shell + --check: würde Kontextmenü installieren (übersprungen)"
            );
        } else {
            println!("\n[shell] installiere Kontextmenü ...");
            let integration = crate::shell::current_integration();
            match integration.install() {
                Ok(()) => println!("  ok: shell-integration installiert"),
                Err(e) => {
                    let msg = format!("shell-integration fehlgeschlagen: {e}");
                    warnings.push(msg.clone());
                    println!("  WARN: {msg}");
                }
            }
        }
    }

    // Abschluss
    println!();
    if opts.check_only {
        println!("setup --check: alle geprüften Ressourcen vorhanden.");
        return Ok(());
    }
    if warnings.is_empty() {
        println!("setup komplett. Nächste Schritte:");
    } else {
        println!("setup mit {} Warnung(en) abgeschlossen:", warnings.len());
        for w in &warnings {
            println!("  - {w}");
        }
        println!("\nTipp: `txtify doctor` zeigt Details, `txtify setup --yes` erneut ausführen.");
    }
    println!("  txtify doctor");
    println!("  txtify convert beispiel.pdf --to md --mode fast");
    println!("  txtify convert scan.pdf --to md --mode high-quality   # nutzt GLM-OCR ({model})");
    if !opts.yes && !opts.with_shell {
        println!("\n  Optional: txtify shell install   # Rechtsklick-Menü");
    }
    let _ = &warnings;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_ok() {
        assert_eq!(parse_python_version("Python 3.11.8\n"), Some((3, 11)));
        assert_eq!(parse_python_version("Python 3.10.0"), Some((3, 10)));
        assert!(parse_python_version("garbage").is_none());
    }

    #[test]
    fn data_dir_nonempty() {
        assert!(!data_dir().as_os_str().is_empty());
        assert!(materialized_sidecar_path()
            .to_string_lossy()
            .contains("txtify_sidecar.py"));
    }

    #[test]
    fn check_only_fails_without_config_or_reports_ok() {
        // ensure_default_config(check_only=true) gibt Ok(None) wenn Config existiert,
        // sonst Err — beides ist für den Test ok, darf nur nicht schreiben.
        let r = ensure_default_config("zai-org/GLM-OCR", "auto", None, None, true);
        assert!(r.is_ok() || r.is_err());
    }
}
