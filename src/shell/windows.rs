use std::path::{Path, PathBuf};

use crate::core::error::TxtifyError;
use crate::shell::{Shell, ShellIntegration};

/// Windows shell implementation for spawning.
pub struct WindowsShell;

impl Shell for WindowsShell {
    fn spawn_sidecar(
        &self,
        program: &str,
        args: &[&str],
    ) -> Result<std::process::Child, TxtifyError> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new(program)
                .args(args)
                .creation_flags(0x0800_0000)
                .spawn()
                .map_err(|e| {
                    TxtifyError::SidecarNotFound(format!("failed to spawn {program}: {e}"))
                })
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new(program)
                .args(args)
                .spawn()
                .map_err(|e| {
                    TxtifyError::SidecarNotFound(format!("failed to spawn {program}: {e}"))
                })
        }
    }
}

/// Windows shell integration via HKCU registry (no admin).
///
/// Creates:
/// - `HKCU\Software\Classes\*\shell\Txtify` with `MUIVerb=Txtify` and
///   `SubCommands=Txtify.FastMd;Txtify.HighQuality;Txtify.Txt`
/// - `HKCU\Software\Classes\*\shell\Txtify.FastMd\command` = `"exe" convert "%1" --to md --mode fast`
/// - `HKCU\Software\Classes\*\shell\Txtify.HighQuality\command` = `"exe" convert "%1" --to md --mode high-quality`
/// - `HKCU\Software\Classes\*\shell\Txtify.Txt\command` = `"exe" convert "%1" --to txt --mode fast`
/// - `HKCU\Software\Classes\Directory\shell\Txtify` (+ subcommands) for batch folders
pub struct WindowsIntegration {
    exe_path: PathBuf,
    /// Base registry path under HKCU, e.g. `Software\Classes`.
    /// For tests this can be overridden to a temp key like `Software\TxtifyTestXXXX\Classes`.
    base_root: String,
}

impl WindowsIntegration {
    /// Create with current executable path and default base `Software\Classes`.
    pub fn new() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("txtify.exe"));
        Self {
            exe_path: exe,
            base_root: r"Software\Classes".to_string(),
        }
    }

    /// Create with explicit exe and base root (used for tests).
    pub fn with_root(exe_path: PathBuf, base_root: impl Into<String>) -> Self {
        Self {
            exe_path,
            base_root: base_root.into(),
        }
    }

    /// Create with custom exe path (production helper).
    pub fn with_exe(exe_path: PathBuf) -> Self {
        Self {
            exe_path,
            base_root: r"Software\Classes".to_string(),
        }
    }

    fn exe_string(&self) -> String {
        // Ensure windows-style quoting; use display string with backslashes.
        self.exe_path.display().to_string()
    }

    fn commands(&self) -> Vec<(String, String, String)> {
        let exe = self.exe_string();
        vec![
            (
                "Txtify.FastMd".to_string(),
                "Convert to Markdown (Fast)".to_string(),
                format!(r#""{exe}" convert "%1" --to md --mode fast"#),
            ),
            (
                "Txtify.HighQuality".to_string(),
                "Convert to Markdown (High Quality)".to_string(),
                format!(r#""{exe}" convert "%1" --to md --mode high-quality"#),
            ),
            (
                "Txtify.Txt".to_string(),
                "Convert to Text (Fast)".to_string(),
                format!(r#""{exe}" convert "%1" --to txt --mode fast"#),
            ),
        ]
    }

    fn batch_commands(&self) -> Vec<(String, String, String)> {
        let exe = self.exe_string();
        vec![
            (
                "Txtify.FastMd".to_string(),
                "Batch to Markdown (Fast)".to_string(),
                format!(r#""{exe}" batch "%1" --to md --mode fast"#),
            ),
            (
                "Txtify.HighQuality".to_string(),
                "Batch to Markdown (High Quality)".to_string(),
                format!(r#""{exe}" batch "%1" --to md --mode high-quality"#),
            ),
            (
                "Txtify.Txt".to_string(),
                "Batch to Text (Fast)".to_string(),
                format!(r#""{exe}" batch "%1" --to txt --mode fast"#),
            ),
        ]
    }

    #[cfg(target_os = "windows")]
    fn install_windows(&self) -> Result<(), TxtifyError> {
        #[cfg(not(feature = "shell"))]
        {
            return Err(TxtifyError::ShellError(
                "shell feature not enabled; build with --features shell".to_string(),
            ));
        }
        #[cfg(feature = "shell")]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;

            let hkcu = RegKey::predef(HKEY_CURRENT_USER);

            // Helper to create key and set values
            let create_parent =
                |subkey: &str, verb: &str, subcmds: &str| -> Result<(), TxtifyError> {
                    let (key, _) = hkcu
                        .create_subkey(subkey)
                        .map_err(|e| TxtifyError::ShellError(format!("create {subkey}: {e}")))?;
                    key.set_value("MUIVerb", &verb)
                        .map_err(|e| TxtifyError::ShellError(e.to_string()))?;
                    key.set_value("SubCommands", &subcmds)
                        .map_err(|e| TxtifyError::ShellError(e.to_string()))?;
                    Ok(())
                };

            let subcommands = "Txtify.FastMd;Txtify.HighQuality;Txtify.Txt";

            // File context menu parent
            let file_parent = format!(r"{}\*\shell\Txtify", self.base_root);
            create_parent(&file_parent, "Txtify", subcommands)?;

            // Directory context menu parent (batch)
            let dir_parent = format!(r"{}\Directory\shell\Txtify", self.base_root);
            create_parent(&dir_parent, "Txtify", subcommands)?;

            // Also Directory\Background for right-click inside folder
            let bg_parent = format!(r"{}\Directory\Background\shell\Txtify", self.base_root);
            // Not critical if fails, try create
            let _ = create_parent(&bg_parent, "Txtify", subcommands);

            // Create file subcommands
            for (id, verb, cmd) in self.commands() {
                let key_path = format!(r"{}\*\shell\{}", self.base_root, id);
                let (key, _) = hkcu
                    .create_subkey(&key_path)
                    .map_err(|e| TxtifyError::ShellError(format!("create {key_path}: {e}")))?;
                key.set_value("MUIVerb", &verb)
                    .map_err(|e| TxtifyError::ShellError(e.to_string()))?;
                if let Ok(icon) = self
                    .exe_path
                    .to_str()
                    .ok_or_else(|| TxtifyError::ShellError("invalid exe".to_string()))
                {
                    let _ = key.set_value("Icon", &format!(r#"{icon},0"#));
                }
                let cmd_path = format!(r"{key_path}\command");
                let (cmd_key, _) = hkcu
                    .create_subkey(&cmd_path)
                    .map_err(|e| TxtifyError::ShellError(format!("create {cmd_path}: {e}")))?;
                cmd_key
                    .set_value("", &cmd)
                    .map_err(|e| TxtifyError::ShellError(e.to_string()))?;
            }

            // Create directory subcommands (batch)
            for (id, verb, cmd) in self.batch_commands() {
                let key_path = format!(r"{}\Directory\shell\{}", self.base_root, id);
                let (key, _) = hkcu
                    .create_subkey(&key_path)
                    .map_err(|e| TxtifyError::ShellError(format!("create {key_path}: {e}")))?;
                key.set_value("MUIVerb", &verb)
                    .map_err(|e| TxtifyError::ShellError(e.to_string()))?;
                let cmd_path = format!(r"{key_path}\command");
                let (cmd_key, _) = hkcu
                    .create_subkey(&cmd_path)
                    .map_err(|e| TxtifyError::ShellError(format!("create {cmd_path}: {e}")))?;
                cmd_key
                    .set_value("", &cmd)
                    .map_err(|e| TxtifyError::ShellError(e.to_string()))?;

                // Also background same command but with %V (folder background)
                let bg_key_path = format!(r"{}\Directory\Background\shell\{}", self.base_root, id);
                if let Ok((bg_key, _)) = hkcu.create_subkey(&bg_key_path) {
                    let _ = bg_key.set_value("MUIVerb", &verb);
                    if let Ok((bg_cmd, _)) = hkcu.create_subkey(format!(r"{bg_key_path}\command")) {
                        let bg_cmd_str = cmd.replace("\"%1\"", "\"%V\"");
                        let _ = bg_cmd.set_value("", &bg_cmd_str);
                    }
                }
            }

            // Background parent already created, ensure its subcommands use %V?
            // Already handled above.

            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            Ok(())
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn install_windows(&self) -> Result<(), TxtifyError> {
        // On non-windows, simulate registry via filesystem for testing.
        // Use base_root as a directory path (e.g. temp dir) to store mock registry.
        // If base_root contains "Software", treat it as mock filesystem under temp.
        // We'll delegate to mock filesystem implementation.
        self.install_mock()
    }

    #[cfg(target_os = "windows")]
    fn uninstall_windows(&self) -> Result<(), TxtifyError> {
        #[cfg(not(feature = "shell"))]
        {
            return Err(TxtifyError::ShellError(
                "shell feature not enabled; build with --features shell".to_string(),
            ));
        }
        #[cfg(feature = "shell")]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);

            let subcommands = ["Txtify.FastMd", "Txtify.HighQuality", "Txtify.Txt"];

            // Remove file entries
            for id in &subcommands {
                let cmd_path = format!(r"{}\*\shell\{}\command", self.base_root, id);
                let key_path = format!(r"{}\*\shell\{}", self.base_root, id);
                let _ = hkcu.delete_subkey_all(&cmd_path);
                let _ = hkcu.delete_subkey_all(&key_path);
            }
            let file_parent = format!(r"{}\*\shell\Txtify", self.base_root);
            let _ = hkcu.delete_subkey_all(&file_parent);

            // Remove directory entries
            for id in &subcommands {
                let cmd_path = format!(r"{}\Directory\shell\{}\command", self.base_root, id);
                let key_path = format!(r"{}\Directory\shell\{}", self.base_root, id);
                let _ = hkcu.delete_subkey_all(&cmd_path);
                let _ = hkcu.delete_subkey_all(&key_path);

                let bg_cmd = format!(
                    r"{}\Directory\Background\shell\{}\command",
                    self.base_root, id
                );
                let bg_key = format!(r"{}\Directory\Background\shell\{}", self.base_root, id);
                let _ = hkcu.delete_subkey_all(&bg_cmd);
                let _ = hkcu.delete_subkey_all(&bg_key);
            }
            let dir_parent = format!(r"{}\Directory\shell\Txtify", self.base_root);
            let _ = hkcu.delete_subkey_all(&dir_parent);
            let bg_parent = format!(r"{}\Directory\Background\shell\Txtify", self.base_root);
            let _ = hkcu.delete_subkey_all(&bg_parent);

            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            Ok(())
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn uninstall_windows(&self) -> Result<(), TxtifyError> {
        self.uninstall_mock()
    }

    #[cfg(target_os = "windows")]
    fn is_installed_windows(&self) -> bool {
        #[cfg(not(feature = "shell"))]
        {
            return false;
        }
        #[cfg(feature = "shell")]
        {
            use winreg::enums::HKEY_CURRENT_USER;
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let file_parent = format!(r"{}\*\shell\Txtify", self.base_root);
            let dir_parent = format!(r"{}\Directory\shell\Txtify", self.base_root);
            let fast_cmd = format!(r"{}\*\shell\Txtify.FastMd\command", self.base_root);
            return hkcu.open_subkey(&file_parent).is_ok()
                && hkcu.open_subkey(&dir_parent).is_ok()
                && hkcu.open_subkey(&fast_cmd).is_ok();
        }
        #[allow(unreachable_code)]
        {
            false
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn is_installed_windows(&self) -> bool {
        self.is_installed_mock()
    }

    // ---- Mock filesystem for non-windows testing ----
    #[cfg(not(target_os = "windows"))]
    fn mock_root_path(&self) -> PathBuf {
        // If base_root is an absolute path or contains '/', use it directly as filesystem path.
        // Otherwise treat as relative under temp dir.
        let p = Path::new(&self.base_root);
        if p.is_absolute() {
            p.to_path_buf()
        } else if self.base_root.contains('/') || self.base_root.contains('\\') {
            // Convert windows separators to native
            let sanitized = self.base_root.replace('\\', "/");
            // If starts with "Software", place under temp dir for isolation
            std::env::temp_dir().join(sanitized)
        } else {
            std::env::temp_dir().join(&self.base_root)
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn install_mock(&self) -> Result<(), TxtifyError> {
        let root = self.mock_root_path();
        // Simulate registry by creating files that represent keys:
        // root/*_shell_Txtify/MUIVerb etc.
        // We'll create directory structure mirroring registry.
        let file_parent = root.join("*").join("shell").join("Txtify");
        std::fs::create_dir_all(&file_parent).map_err(TxtifyError::Io)?;
        std::fs::write(file_parent.join("MUIVerb"), "Txtify").map_err(TxtifyError::Io)?;
        std::fs::write(
            file_parent.join("SubCommands"),
            "Txtify.FastMd;Txtify.HighQuality;Txtify.Txt",
        )
        .map_err(TxtifyError::Io)?;

        let dir_parent = root.join("Directory").join("shell").join("Txtify");
        std::fs::create_dir_all(&dir_parent).map_err(TxtifyError::Io)?;
        std::fs::write(dir_parent.join("MUIVerb"), "Txtify").map_err(TxtifyError::Io)?;
        std::fs::write(
            dir_parent.join("SubCommands"),
            "Txtify.FastMd;Txtify.HighQuality;Txtify.Txt",
        )
        .map_err(TxtifyError::Io)?;

        let bg_parent = root
            .join("Directory")
            .join("Background")
            .join("shell")
            .join("Txtify");
        let _ = std::fs::create_dir_all(&bg_parent);

        for (id, verb, cmd) in self.commands() {
            let key_path = root.join("*").join("shell").join(&id);
            std::fs::create_dir_all(&key_path).map_err(TxtifyError::Io)?;
            std::fs::write(key_path.join("MUIVerb"), verb).map_err(TxtifyError::Io)?;
            let cmd_dir = key_path.join("command");
            std::fs::create_dir_all(&cmd_dir).map_err(TxtifyError::Io)?;
            std::fs::write(cmd_dir.join("default"), cmd).map_err(TxtifyError::Io)?;
        }
        for (id, verb, cmd) in self.batch_commands() {
            let key_path = root.join("Directory").join("shell").join(&id);
            std::fs::create_dir_all(&key_path).map_err(TxtifyError::Io)?;
            std::fs::write(key_path.join("MUIVerb"), &verb).map_err(TxtifyError::Io)?;
            let cmd_dir = key_path.join("command");
            std::fs::create_dir_all(&cmd_dir).map_err(TxtifyError::Io)?;
            std::fs::write(cmd_dir.join("default"), &cmd).map_err(TxtifyError::Io)?;

            let bg_key = root
                .join("Directory")
                .join("Background")
                .join("shell")
                .join(&id);
            if std::fs::create_dir_all(&bg_key).is_ok() {
                let _ = std::fs::write(bg_key.join("MUIVerb"), &verb);
                let bg_cmd_dir = bg_key.join("command");
                if std::fs::create_dir_all(&bg_cmd_dir).is_ok() {
                    let _ =
                        std::fs::write(bg_cmd_dir.join("default"), cmd.replace("\"%1\"", "\"%V\""));
                }
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn uninstall_mock(&self) -> Result<(), TxtifyError> {
        let root = self.mock_root_path();
        if root.exists() {
            // Remove the whole mock hive if it's a temp test root (contains TxtifyTest)
            // Otherwise only remove the specific subkeys.
            if self.base_root.contains("TxtifyTest") {
                let _ = std::fs::remove_dir_all(&root);
            } else {
                for id in ["Txtify.FastMd", "Txtify.HighQuality", "Txtify.Txt"] {
                    let _ = std::fs::remove_dir_all(root.join("*").join("shell").join(id));
                    let _ = std::fs::remove_dir_all(root.join("Directory").join("shell").join(id));
                    let _ = std::fs::remove_dir_all(
                        root.join("Directory")
                            .join("Background")
                            .join("shell")
                            .join(id),
                    );
                }
                let _ = std::fs::remove_dir_all(root.join("*").join("shell").join("Txtify"));
                let _ =
                    std::fs::remove_dir_all(root.join("Directory").join("shell").join("Txtify"));
                let _ = std::fs::remove_dir_all(
                    root.join("Directory")
                        .join("Background")
                        .join("shell")
                        .join("Txtify"),
                );
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn is_installed_mock(&self) -> bool {
        let root = self.mock_root_path();
        let file_parent = root.join("*").join("shell").join("Txtify");
        let dir_parent = root.join("Directory").join("shell").join("Txtify");
        let fast_cmd = root
            .join("*")
            .join("shell")
            .join("Txtify.FastMd")
            .join("command")
            .join("default");
        file_parent.exists() && dir_parent.exists() && fast_cmd.exists()
    }
}

impl Default for WindowsIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellIntegration for WindowsIntegration {
    fn install(&self) -> Result<(), TxtifyError> {
        self.install_windows()
    }

    fn uninstall(&self) -> Result<(), TxtifyError> {
        self.uninstall_windows()
    }

    fn is_installed(&self) -> bool {
        self.is_installed_windows()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_commands_format() {
        let wi = WindowsIntegration::with_root(
            PathBuf::from(r"C:\path\txtify.exe"),
            r"Software\Classes",
        );
        let cmds = wi.commands();
        assert_eq!(cmds.len(), 3);
        assert_eq!(
            cmds[0].2,
            r#""C:\path\txtify.exe" convert "%1" --to md --mode fast"#
        );
        assert_eq!(
            cmds[1].2,
            r#""C:\path\txtify.exe" convert "%1" --to md --mode high-quality"#
        );
        assert_eq!(
            cmds[2].2,
            r#""C:\path\txtify.exe" convert "%1" --to txt --mode fast"#
        );

        let batch = wi.batch_commands();
        assert_eq!(
            batch[0].2,
            r#""C:\path\txtify.exe" batch "%1" --to md --mode fast"#
        );
    }

    #[test]
    fn test_mock_install_uninstall_is_installed() {
        // Use a temp directory as mock registry root
        let tmp = tempfile::tempdir().expect("tempdir");
        let mock_root = tmp.path().join("MockHive").to_string_lossy().to_string();
        // For non-windows, base_root is treated as filesystem path if absolute
        let exe = PathBuf::from(r"C:\path\txtify.exe");
        let wi = WindowsIntegration::with_root(exe, mock_root.clone());

        assert!(!wi.is_installed(), "should not be installed initially");
        wi.install().expect("install");
        assert!(wi.is_installed(), "should be installed after install");

        // Verify mock files
        let root = PathBuf::from(&mock_root);
        let verbs =
            std::fs::read_to_string(root.join("*").join("shell").join("Txtify").join("MUIVerb"))
                .expect("MUIVerb");
        assert_eq!(verbs, "Txtify");
        let sc = std::fs::read_to_string(
            root.join("*")
                .join("shell")
                .join("Txtify")
                .join("SubCommands"),
        )
        .expect("SubCommands");
        assert_eq!(sc, "Txtify.FastMd;Txtify.HighQuality;Txtify.Txt");

        let fast_cmd = std::fs::read_to_string(
            root.join("*")
                .join("shell")
                .join("Txtify.FastMd")
                .join("command")
                .join("default"),
        )
        .expect("fast cmd");
        assert_eq!(
            fast_cmd,
            r#""C:\path\txtify.exe" convert "%1" --to md --mode fast"#
        );

        // Directory batch
        let dir_fast = std::fs::read_to_string(
            root.join("Directory")
                .join("shell")
                .join("Txtify.FastMd")
                .join("command")
                .join("default"),
        )
        .expect("dir fast");
        assert_eq!(
            dir_fast,
            r#""C:\path\txtify.exe" batch "%1" --to md --mode fast"#
        );

        wi.uninstall().expect("uninstall");
        assert!(
            !wi.is_installed(),
            "should not be installed after uninstall"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_registry_temp_key() {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let exe = PathBuf::from(r"C:\tmp\txtify.exe");
        let uuid = format!(
            "Software\\TxtifyTest_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = format!(r"{}\Software\Classes", uuid);
        // Ensure base doesn't interfere: use uuid as top level and then Software\Classes under it
        // Simpler: use uuid directly as base_root that includes Software\Classes
        let wi = WindowsIntegration::with_root(exe.clone(), base.clone());
        // Ensure clean
        let _ = wi.uninstall();
        assert!(!wi.is_installed());
        wi.install().expect("install temp key");

        assert!(wi.is_installed());

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let parent: String = hkcu
            .open_subkey(format!(r"{}\*\shell\Txtify", base))
            .expect("parent exists")
            .get_value("MUIVerb")
            .expect("MUIVerb");
        assert_eq!(parent, "Txtify");
        let sc: String = hkcu
            .open_subkey(format!(r"{}\*\shell\Txtify", base))
            .unwrap()
            .get_value("SubCommands")
            .unwrap();
        assert_eq!(sc, "Txtify.FastMd;Txtify.HighQuality;Txtify.Txt");

        let cmd: String = hkcu
            .open_subkey(format!(r"{}\*\shell\Txtify.FastMd\command", base))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(
            cmd,
            r#""C:\tmp\txtify.exe" convert "%1" --to md --mode fast"#
        );

        let dir_cmd: String = hkcu
            .open_subkey(format!(r"{}\Directory\shell\Txtify.FastMd\command", base))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(
            dir_cmd,
            r#""C:\tmp\txtify.exe" batch "%1" --to md --mode high-quality"#
                .replace("high-quality", "fast")
        ); // first is fast

        wi.uninstall().expect("uninstall");
        assert!(!wi.is_installed());

        // Cleanup top level temp key
        let top = uuid.split('\\').next().unwrap().to_string();
        // Actually uuid is like Software\TxtifyTest_xxx\Software\Classes -> top is Software
        // So we need to delete the specific TxtifyTest key
        let test_key = uuid.split('\\').take(2).collect::<Vec<_>>().join("\\");
        let _ = hkcu.delete_subkey_all(&test_key);
    }
}
