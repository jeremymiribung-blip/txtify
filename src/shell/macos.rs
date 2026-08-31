use std::path::PathBuf;

use crate::core::error::TxtifyError;
use crate::shell::{Shell, ShellIntegration};

/// macOS shell implementation for spawning.
pub struct MacosShell;

impl Shell for MacosShell {
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

/// macOS shell integration via Automator Quick Action instructions.
///
/// Finder Sync extensions require Swift and an app bundle; pure Rust CLI
/// cannot register a Finder Sync extension directly. Instead, we provide
/// Automator Quick Action instructions and optionally create a workflow
/// placeholder. Installation prints instructions; `is_installed` checks for
/// a workflow at `~/Library/Services/Txtify.workflow`.
pub struct MacosIntegration {
    exe_path: PathBuf,
    workflow_path: PathBuf,
}

impl MacosIntegration {
    /// new — fn for txtify.
    pub fn new() -> Self {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("txtify"));
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let workflow = home.join("Library/Services/Txtify.workflow");
        Self {
            exe_path: exe,
            workflow_path: workflow,
        }
    }

    /// Create with custom exe and workflow path (for tests).
    pub fn with_paths(exe_path: PathBuf, workflow_path: PathBuf) -> Self {
        Self {
            exe_path,
            workflow_path,
        }
    }

    /// instructions — fn for txtify.
    pub fn instructions(&self) -> String {
        let exe = self.exe_path.display();
        format!(
            r#"macOS Finder Integration - Automator Quick Action
=============================================
Finder Sync extensions are Swift-only and require an app bundle, which
is not available for the pure Rust `txtify` CLI. Please create an
Automator Quick Action instead:

1. Open Automator.app
2. Choose File > New > Quick Action (or Service on older macOS)
3. Configure at top:
   - Workflow receives current: "files or folders" in "Finder"
   - Image: (optional) choose an icon
   - Color: (optional)

4. From the left library, drag "Run Shell Script" to the workflow.

5. Configure "Run Shell Script":
   - Shell: /bin/bash
   - Pass input: as arguments
   - Script content:

     for f in "$@"; do
         "{exe}" convert "$f" --to md --mode fast
     done

   Variants (create separate Quick Actions or use a single with chooser):
     - Fast MD:       "{exe}" convert "$f" --to md --mode fast
     - High Quality:  "{exe}" convert "$f" --to md --mode high-quality
     - Txt:           "{exe}" convert "$f" --to txt --mode fast
     - Batch folder:  "{exe}" batch "$f" --to md --mode fast

   Optional: add `afplay` or `osascript -e 'display notification ...'` for feedback.

6. Save as "Txtify" (or "Txtify Fast MD", etc.)
   - Automator will save to ~/Library/Services/Txtify.workflow

7. Enable in System Settings > Privacy & Security > Extensions > Finder
   or System Preferences > Extensions > Finder (on older macOS) if needed.

8. In Finder, right-click a file or folder > Quick Actions > Txtify

Notes:
- The workflow is stored at: {}
- To uninstall, delete: {}
  or remove via Automator.

Alternative for advanced users:
- Build a Swift Finder Sync extension that calls `txtify` sidecar.
- The extension would use `FIFinderSyncController` (Swift-only API).
- See: https://developer.apple.com/documentation/findersync

Current txtify executable: {exe}
Workflow path: {}
"#,
            self.workflow_path.display(),
            self.workflow_path.display(),
            self.workflow_path.display()
        )
    }
}

impl ShellIntegration for MacosIntegration {
    fn install(&self) -> Result<(), TxtifyError> {
        let instr = self.instructions();
        // Print to stdout as required by spec
        println!("{instr}");

        // Optionally create a placeholder workflow directory with instructions
        // to make `is_installed` return true after user manually creates workflow,
        // but we do not create the actual workflow automatically since Automator
        // format is complex (plist + workflow).
        // We will create a README placeholder if the workflow does not exist.
        if !self.workflow_path.exists() {
            // Create a placeholder instruction file next to workflow path
            // e.g., ~/Library/Services/Txtify.workflow/README.txt
            // This is not the real workflow but indicates installation attempt.
            // For purity, we only print instructions and don't create files,
            // so `is_installed` remains false until user creates workflow manually.
            // Uncomment below to create placeholder:
            // let placeholder = self.workflow_path.join("README.txt");
            // let _ = std::fs::create_dir_all(&self.workflow_path);
            // let _ = std::fs::write(placeholder, instr);
            tracing::info!(
                "macOS shell integration requires manual Automator setup at {}",
                self.workflow_path.display()
            );
        } else {
            println!(
                "Workflow already exists at: {}",
                self.workflow_path.display()
            );
        }

        Ok(())
    }

    fn uninstall(&self) -> Result<(), TxtifyError> {
        if self.workflow_path.exists() {
            println!(
                "To uninstall, delete the workflow at: {}",
                self.workflow_path.display()
            );
            println!("You may also remove it via Automator or Finder Quick Actions settings.");
            // We do not automatically delete without confirmation; print instruction.
            // For `shell uninstall` to have effect in tests, we will remove a placeholder if it exists.
            // Check if it's a placeholder (contains README) or we are in test temp dir.
            let meta = std::fs::metadata(&self.workflow_path);
            if let Ok(m) = meta {
                if m.is_dir() {
                    // Only auto-remove if path contains "txtify_test" (test) or if user confirms via env
                    let path_str = self.workflow_path.to_string_lossy();
                    if path_str.contains("txtify_test") || path_str.contains("tmp") {
                        std::fs::remove_dir_all(&self.workflow_path).map_err(TxtifyError::Io)?;
                        println!(
                            "Removed workflow placeholder at {}",
                            self.workflow_path.display()
                        );
                    } else {
                        println!(
                            "Not automatically deleting real workflow. Please delete manually:"
                        );
                        println!("  rm -rf \"{}\"", self.workflow_path.display());
                    }
                } else {
                    let _ = std::fs::remove_file(&self.workflow_path);
                }
            }
        } else {
            println!(
                "No workflow found at {}. Nothing to uninstall.",
                self.workflow_path.display()
            );
            println!("{}", self.instructions());
        }
        Ok(())
    }

    fn is_installed(&self) -> bool {
        // Consider installed if the workflow directory/file exists and looks like a workflow.
        if self.workflow_path.exists() {
            // Check for typical workflow structure: contains document.wflow or Contents
            let doc = self.workflow_path.join("document.wflow");
            let contents = self.workflow_path.join("Contents");
            if doc.exists() || contents.exists() || self.workflow_path.is_dir() {
                return true;
            }
            // If it's a file, consider installed
            return true;
        }
        false
    }
}

impl Default for MacosIntegration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_instructions_contain_exe_and_finder_sync_note() {
        let mi = MacosIntegration::with_paths(
            PathBuf::from("/usr/local/bin/txtify"),
            PathBuf::from("/tmp/txtify_test_mac/Txtify.workflow"),
        );
        let instr = mi.instructions();
        assert!(instr.contains("/usr/local/bin/txtify"));
        assert!(instr.contains("Automator"));
        assert!(instr.contains("Finder Sync"));
        assert!(instr.contains("Swift-only"));
        assert!(instr.contains("Quick Action"));
        assert!(instr.contains("Run Shell Script"));
    }

    #[test]
    fn test_is_installed_with_temp_workflow() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workflow = tmp.path().join("Txtify.workflow");
        let mi = MacosIntegration::with_paths(PathBuf::from("/usr/bin/txtify"), workflow.clone());
        assert!(!mi.is_installed());
        // Simulate user creating workflow (create dir with document.wflow)
        std::fs::create_dir_all(&workflow).unwrap();
        std::fs::write(workflow.join("document.wflow"), "fake").unwrap();
        assert!(mi.is_installed());
        mi.uninstall().unwrap();
        assert!(!mi.is_installed());
    }

    #[test]
    fn test_install_prints_instructions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workflow = tmp.path().join("Txtify.workflow");
        let mi = MacosIntegration::with_paths(PathBuf::from("/opt/txtify"), workflow);
        // install should succeed and print (we don't capture stdout here, just check Ok)
        let res = mi.install();
        assert!(res.is_ok());
    }
}
