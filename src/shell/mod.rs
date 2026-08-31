/// linux — mod for txtify.
pub mod linux;
/// macos — mod for txtify.
pub mod macos;
/// windows — mod for txtify.
pub mod windows;

use crate::core::error::TxtifyError;

/// Shell abstraction for spawning sidecar processes.
pub trait Shell: Send + Sync {
    fn spawn_sidecar(
        &self,
        program: &str,
        args: &[&str],
    ) -> Result<std::process::Child, TxtifyError>;
}

/// Shell integration for OS context menus / file manager actions.
///
/// Implementations must use HKCU (no admin) on Windows, `.desktop` + nautilus
/// script on Linux, and Automator instructions on macOS.
pub trait ShellIntegration: Send + Sync {
    fn install(&self) -> Result<(), TxtifyError>;
    fn uninstall(&self) -> Result<(), TxtifyError>;
    fn is_installed(&self) -> bool;
}

/// Platform-specific shell implementation.
pub fn current_shell() -> Box<dyn Shell> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsShell)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxShell)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosShell)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Box::new(generic_shell::GenericShell)
    }
}

/// Returns platform-specific `ShellIntegration`.
///
/// When the `shell` feature is disabled, returns a stub that errors on install.
pub fn current_integration() -> Box<dyn ShellIntegration> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsIntegration::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxIntegration::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosIntegration::new())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Box::new(generic_shell::GenericIntegration)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod generic_shell {
    use super::Shell;
    use super::ShellIntegration;
    use crate::core::error::TxtifyError;

    /// GenericShell — struct for txtify.
    pub struct GenericShell;

    impl Shell for GenericShell {
        fn spawn_sidecar(
            &self,
            program: &str,
            args: &[&str],
        ) -> Result<std::process::Child, TxtifyError> {
            std::process::Command::new(program)
                .args(args)
                .spawn()
                .map_err(|e| {
                    TxtifyError::SidecarNotFound(format!("failed to spawn {program}: {e}"))
                })
        }
    }

    /// GenericIntegration — struct for txtify.
    pub struct GenericIntegration;

    impl ShellIntegration for GenericIntegration {
        fn install(&self) -> Result<(), TxtifyError> {
            Err(TxtifyError::ShellError(
                "shell integration not supported on this platform".to_string(),
            ))
        }

        fn uninstall(&self) -> Result<(), TxtifyError> {
            Err(TxtifyError::ShellError(
                "shell integration not supported on this platform".to_string(),
            ))
        }

        fn is_installed(&self) -> bool {
            false
        }
    }
}
