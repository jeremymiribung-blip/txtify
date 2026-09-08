use std::path::PathBuf;

use crate::core::error::TxtifyError;
use crate::shell::{Shell, ShellIntegration};

/// Linux shell implementation for spawning.
pub struct LinuxShell;

impl Shell for LinuxShell {
    fn spawn_sidecar(
        &self,
        program: &str,
        args: &[&str],
    ) -> Result<std::process::Child, TxtifyError> {
        std::process::Command::new(program)
            .args(args)
            .spawn()
            .map_err(|e| TxtifyError::SidecarNotFound(format!("failed to spawn {program}: {e}")))
    }
}

/// Linux shell integration via `.desktop` + Nautilus script.
///
/// Creates:
/// - `~/.local/share/file-manager/actions/txtify.desktop` (or custom base)
/// - `~/.local/share/nautilus/scripts/Txtify*` scripts
///
/// Also supports `~/.local/share/kio/servicemenus/` for KDE Dolphin as secondary.
pub struct LinuxIntegration {
    exe_path: PathBuf,
    actions_file: PathBuf,
    nautilus_scripts: Vec<PathBuf>,
    dolphin_file: Option<PathBuf>,
}

impl LinuxIntegration {
    /// Create with default XDG paths based on `$HOME` or `$XDG_DATA_HOME`.
    pub fn new() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("txtify"));
        let home = dirs_home();
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".local/share"));

        let actions = data_home.join("file-manager/actions/txtify.desktop");
        let nautilus_base = data_home.join("nautilus/scripts");
        let scripts = vec![
            nautilus_base.join("Txtify Fast MD"),
            nautilus_base.join("Txtify High Quality"),
            nautilus_base.join("Txtify Txt"),
        ];
        // Also single wrapper for backward compat
        // scripts.push(nautilus_base.join("Txtify"));

        let dolphin = data_home.join("kio/servicemenus/txtify.desktop");

        Self {
            exe_path: exe,
            actions_file: actions,
            nautilus_scripts: scripts,
            dolphin_file: Some(dolphin),
        }
    }

    /// Create with custom base directory (used for tests).
    ///
    /// `base` will be used as `$XDG_DATA_HOME`. Paths become:
    /// - `base/file-manager/actions/txtify.desktop`
    /// - `base/nautilus/scripts/Txtify*`
    pub fn with_base(exe_path: PathBuf, base: impl Into<PathBuf>) -> Self {
        let base = base.into();
        let actions = base.join("file-manager/actions/txtify.desktop");
        let nautilus_base = base.join("nautilus/scripts");
        let scripts = vec![
            nautilus_base.join("Txtify Fast MD"),
            nautilus_base.join("Txtify High Quality"),
            nautilus_base.join("Txtify Txt"),
        ];
        let dolphin = base.join("kio/servicemenus/txtify.desktop");
        Self {
            exe_path,
            actions_file: actions,
            nautilus_scripts: scripts,
            dolphin_file: Some(dolphin),
        }
    }

    /// Create with explicit actions file and script directory (fine-grained for tests).
    pub fn with_paths(
        exe_path: PathBuf,
        actions_file: PathBuf,
        nautilus_scripts: Vec<PathBuf>,
    ) -> Self {
        Self {
            exe_path,
            actions_file,
            nautilus_scripts,
            dolphin_file: None,
        }
    }

    fn desktop_content(&self) -> String {
        let exe = self.exe_path.display();
        format!(
            r#"[Desktop Entry]
Type=Action
ToolbarLabel=Txtify
Name=Txtify
Tooltip=Convert documents with Txtify
Icon=text-plain
Profiles=profile-zero;

[X-Action-Profile profile-zero]
MimeTypes=all/allfiles;inode/directory;
Exec={exe} convert %F --to md --mode fast
Name=Txtify
SelectionCount=>0

[Desktop Action FastMd]
Name=Convert to Markdown (Fast)
Exec={exe} convert %F --to md --mode fast
Icon=text-markdown

[Desktop Action HighQuality]
Name=Convert to Markdown (High Quality)
Exec={exe} convert %F --to md --mode high-quality
Icon=text-markdown

[Desktop Action Txt]
Name=Convert to Text (Fast)
Exec={exe} convert %F --to txt --mode fast
Icon=text-plain
"#
        )
    }

    fn dolphin_content(&self) -> String {
        let exe = self.exe_path.display();
        format!(
            r#"[Desktop Entry]
Type=Service
ServiceTypes=KonqPopupMenu/Plugin
MimeType=all/allfiles;inode/directory;
Actions=TxtifyFastMd;TxtifyHighQuality;TxtifyTxt;
X-KDE-Priority=TopLevel

[Desktop Action TxtifyFastMd]
Name=Txtify: Convert to MD (Fast)
Exec={exe} convert %F --to md --mode fast
Icon=text-markdown

[Desktop Action TxtifyHighQuality]
Name=Txtify: Convert to MD (High Quality)
Exec={exe} convert %F --to md --mode high-quality
Icon=text-markdown

[Desktop Action TxtifyTxt]
Name=Txtify: Convert to TXT (Fast)
Exec={exe} convert %F --to txt --mode fast
Icon=text-plain
"#
        )
    }

    fn nautilus_script_content(&self, mode: &str) -> String {
        let exe = self.exe_path.display();
        let (to, mode_flag, label) = match mode {
            "fast_md" => ("md", "fast", "Fast MD"),
            "high_quality" => ("md", "high-quality", "High Quality MD"),
            "txt" => ("txt", "fast", "TXT"),
            _ => ("md", "fast", "Fast MD"),
        };
        // High-Quality braucht auf CPU Minuten pro Seite: Skript kehrt sofort
        // zurück, die Arbeit läuft entkoppelt im Hintergrund (nohup), Start-
        // und Fertig-Meldung via Notification-Daemon (kein Terminal sichtbar).
        let launch = if mode == "high_quality" {
            r#"if [ "${1:-}" != "--txtify-bg" ]; then
    txtify_notify "Txtify ({label})" "Gestartet – dauert auf CPU einige Minuten pro Seite."
    mkdir -p "${HOME}/.cache"
    nohup "$0" --txtify-bg "$@" >> "${HOME}/.cache/txtify-hq.log" 2>&1 &
    exit 0
fi
shift
run_all
txtify_notify "Txtify ({label})" "Fertig – die Ausgabe liegt neben der Datei."
"#
            .replace("{label}", label)
        } else {
            r#"run_all
txtify_notify "Txtify ({label})" "Fertig – die Ausgabe liegt neben der Datei."
"#
            .replace("{label}", label)
        };
        format!(
            r#"#!/bin/bash
# Txtify Nautilus script - {label}
# Generated by `txtify shell install`
set -e
EXE="{exe}"

# Nautilus provides selected files via $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS (newline separated)
# Fallback to arguments and $NAUTILUS_SCRIPT_SELECTED_URIS
# Output is written next to the input file (single files print to stdout
# by default, which would be lost without a terminal).
handle_file() {{
    local file="$1"
    if [ -d "$file" ]; then
        "$EXE" batch "$file" --to {to} --mode {mode_flag} --overwrite
    else
        "$EXE" convert "$file" -o "${{file%.*}}.{to}" --to {to} --mode {mode_flag} --overwrite
    fi
}}

# Benachrichtigung ohne Terminal: gdbus direkt an den Notification-Daemon
# (das notify-send-Binary ist auf manchen Systemen defekt), Fallback notify-send.
txtify_notify() {{
    local title="$1"
    local body="$2"
    if command -v gdbus >/dev/null 2>&1; then
        gdbus call --session --dest org.freedesktop.Notifications \
            --object-path /org/freedesktop/Notifications \
            --method org.freedesktop.Notifications.Notify \
            "Txtify" 0 "" "$title" "$body" '[]' '{{}}' 5000 >/dev/null 2>&1 || true
    elif command -v notify-send >/dev/null 2>&1; then
        notify-send "$title" "$body" || true
    fi
}}

run_all() {{
if [ -n "$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS" ]; then
    echo "$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS" | while IFS= read -r file; do
        [ -z "$file" ] && continue
        handle_file "$file"
    done
elif [ -n "$NAUTILUS_SCRIPT_SELECTED_URIS" ]; then
    echo "$NAUTILUS_SCRIPT_SELECTED_URIS" | while IFS= read -r uri; do
        [ -z "$uri" ] && continue
        # uri is file://...
        file=$(echo "$uri" | sed 's|^file://||' | sed 's|%20| |g')
        handle_file "$file"
    done
elif [ $# -gt 0 ]; then
    for file in "$@"; do
        handle_file "$file"
    done
else
    # If no selection, try current directory
    handle_file "$(pwd)"
fi
}}

{launch}"#,
        )
    }
}

impl ShellIntegration for LinuxIntegration {
    fn install(&self) -> Result<(), TxtifyError> {
        // file-manager actions .desktop
        if let Some(parent) = self.actions_file.parent() {
            std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
        }
        std::fs::write(&self.actions_file, self.desktop_content()).map_err(TxtifyError::Io)?;

        // dolphin / kio servicemenus if available
        if let Some(ref dolphin) = self.dolphin_file {
            if let Some(parent) = dolphin.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(dolphin, self.dolphin_content());
        }

        // nautilus scripts
        let modes = ["fast_md", "high_quality", "txt"];
        for (script_path, mode) in self.nautilus_scripts.iter().zip(modes.iter()) {
            if let Some(parent) = script_path.parent() {
                std::fs::create_dir_all(parent).map_err(TxtifyError::Io)?;
            }
            let content = self.nautilus_script_content(mode);
            std::fs::write(script_path, content).map_err(TxtifyError::Io)?;
            // chmod +x
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(script_path)
                    .map_err(TxtifyError::Io)?
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(script_path, perms).map_err(TxtifyError::Io)?;
            }
        }

        // Also ensure nautilus scripts have correct handling for single wrapper case if only one script expected.
        // If only one script path is configured (legacy), we already handled above.

        Ok(())
    }

    fn uninstall(&self) -> Result<(), TxtifyError> {
        let _ = std::fs::remove_file(&self.actions_file);
        if let Some(ref dolphin) = self.dolphin_file {
            let _ = std::fs::remove_file(dolphin);
        }
        for script in &self.nautilus_scripts {
            let _ = std::fs::remove_file(script);
        }
        // Try to clean up empty parent dirs (ignore errors)
        if let Some(parent) = self.actions_file.parent() {
            let _ = std::fs::remove_dir(parent);
            if let Some(grand) = parent.parent() {
                let _ = std::fs::remove_dir(grand);
            }
        }
        Ok(())
    }

    fn is_installed(&self) -> bool {
        let actions_exists = self.actions_file.exists();
        // Consider installed if at least actions file OR any nautilus script exists.
        // Strict: require actions file; lenient: actions file is primary.
        // For status, we check actions file and at least one script.
        let any_script = self.nautilus_scripts.iter().any(|p| p.exists());
        // If nautilus scripts vec is empty, just check actions file
        if self.nautilus_scripts.is_empty() {
            actions_exists
        } else {
            actions_exists && any_script
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl Default for LinuxIntegration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_desktop_content_contains_commands() {
        let li = LinuxIntegration::with_base(
            PathBuf::from("/usr/local/bin/txtify"),
            PathBuf::from("/tmp/txtify_test_desktop"),
        );
        let content = li.desktop_content();
        assert!(content.contains(r#"/usr/local/bin/txtify convert %F --to md --mode fast"#));
        assert!(content.contains(r#"--to md --mode high-quality"#));
        assert!(content.contains(r#"--to txt --mode fast"#));
        assert!(content.contains("Txtify"));
    }

    #[test]
    fn test_nautilus_script_content() {
        let li = LinuxIntegration::with_base(
            PathBuf::from("/opt/txtify"),
            PathBuf::from("/tmp/txtify_test_nautilus"),
        );
        let c = li.nautilus_script_content("fast_md");
        assert!(c.contains(r#""/opt/txtify" convert"#) || c.contains(r#"EXE="/opt/txtify""#));
        assert!(c.contains("--to md --mode fast"));
        // Output must go to a file next to the input (stdout is lost without terminal)
        assert!(c.contains("-o \"${file%.*}.md\""));
        let hq = li.nautilus_script_content("high_quality");
        assert!(hq.contains("--mode high-quality"));
        // HQ runs detached (CPU inference takes minutes) with notifications
        assert!(hq.contains("--txtify-bg"));
        assert!(hq.contains("txtify_notify"));
    }

    #[test]
    fn test_install_uninstall_is_installed_with_temp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().to_path_buf();
        let exe = PathBuf::from("/usr/bin/txtify");
        let li = LinuxIntegration::with_base(exe, base.clone());

        assert!(!li.is_installed());
        li.install().expect("install");
        assert!(li.is_installed());

        // check files
        let actions = base.join("file-manager/actions/txtify.desktop");
        assert!(actions.exists());
        let content = std::fs::read_to_string(&actions).unwrap();
        assert!(content.contains("/usr/bin/txtify"));

        for script in &li.nautilus_scripts {
            assert!(script.exists(), "script {:?} should exist", script);
            let s = std::fs::read_to_string(script).unwrap();
            assert!(s.starts_with("#!/bin/bash"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(script).unwrap().permissions().mode();
                assert!(mode & 0o111 != 0, "executable bit");
            }
        }

        // check dolphin
        let dolphin = base.join("kio/servicemenus/txtify.desktop");
        assert!(dolphin.exists());

        li.uninstall().expect("uninstall");
        assert!(!li.is_installed());
        assert!(!actions.exists());
        for s in &li.nautilus_scripts {
            assert!(!s.exists());
        }
    }
}
