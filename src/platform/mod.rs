use std::ffi::OsStr;

use crate::internal::hanafubuki_app_update::{AppArchitecture, AppPlatform};
use crate::internal::hanafubuki_foundation::paths::{PathError, ResolvedPath};
use crate::internal::hanafubuki_process::executor::ExecutorManager;

#[cfg(unix)]
mod command;
#[cfg(any(target_os = "linux", test))]
mod linux;
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(windows)]
pub(crate) mod windows;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LauncherInstallKind {
    File,
    MacApp,
}

#[derive(Clone, Debug)]
pub struct LauncherLayout {
    pub executable: ResolvedPath,
    pub install_target: ResolvedPath,
    pub portable_root: ResolvedPath,
    pub install_kind: LauncherInstallKind,
}

pub fn system_proxy_address(executor: &ExecutorManager) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = executor;
        return windows::system_proxy_address();
    }
    #[cfg(target_os = "linux")]
    return linux::detect(executor);
    #[cfg(target_os = "macos")]
    return macos::detect(executor);
    #[allow(unreachable_code)]
    Ok(None)
}

pub fn launcher_layout() -> Result<LauncherLayout, String> {
    let executable = ResolvedPath::resolve(
        std::env::current_exe().map_err(|error| format!("Find launcher executable: {error}"))?,
    )
    .map_err(|error| error.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let macos = resolved_parent(&executable, "launcher executable")?;
        let contents = resolved_parent(&macos, "MacOS directory")?;
        let app = resolved_parent(&contents, "Contents directory")?;
        if macos.file_name() != Some(OsStr::new("MacOS"))
            || contents.file_name() != Some(OsStr::new("Contents"))
            || app.file_name() != Some(OsStr::new("Hanafubuki Launcher.app"))
        {
            return Err(
                "The macOS launcher must run from Hanafubuki Launcher.app/Contents/MacOS"
                    .to_string(),
            );
        }
        let portable_root = resolved_parent(&app, "launcher application")?;
        return Ok(LauncherLayout {
            executable,
            install_target: app,
            portable_root,
            install_kind: LauncherInstallKind::MacApp,
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let portable_root = resolved_parent(&executable, "launcher executable")?;
        Ok(LauncherLayout {
            install_target: executable.clone(),
            executable,
            portable_root,
            install_kind: LauncherInstallKind::File,
        })
    }
}

fn resolved_parent(path: &ResolvedPath, label: &str) -> Result<ResolvedPath, String> {
    let parent = path
        .native()
        .parent()
        .ok_or_else(|| format!("The {label} does not have a parent directory"))?;
    ResolvedPath::resolve(parent).map_err(|error| error.to_string())
}

#[derive(Clone)]
pub struct TargetSpec {
    pub platform: AppPlatform,
    pub architecture: AppArchitecture,
    pub package_kind: &'static str,
    executable_components: &'static [&'static str],
}

pub fn current_target() -> Result<TargetSpec, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok(TargetSpec {
            platform: AppPlatform::Windows,
            architecture: AppArchitecture::X86_64,
            package_kind: "portable-zip",
            executable_components: &["Hanafubuki", "hanafubuki.exe"],
        }),
        ("macos", "x86_64") => Ok(TargetSpec {
            platform: AppPlatform::Macos,
            architecture: AppArchitecture::X86_64,
            package_kind: "app-tar-gz",
            executable_components: &["Hanafubuki.app", "Contents", "MacOS", "hanafubuki"],
        }),
        ("macos", "aarch64") => Ok(TargetSpec {
            platform: AppPlatform::Macos,
            architecture: AppArchitecture::Aarch64,
            package_kind: "app-tar-gz",
            executable_components: &["Hanafubuki.app", "Contents", "MacOS", "hanafubuki"],
        }),
        ("linux", "x86_64") => Ok(TargetSpec {
            platform: AppPlatform::Linux,
            architecture: AppArchitecture::X86_64,
            package_kind: "appimage",
            executable_components: &["Hanafubuki.AppImage"],
        }),
        ("linux", "aarch64") => Ok(TargetSpec {
            platform: AppPlatform::Linux,
            architecture: AppArchitecture::Aarch64,
            package_kind: "appimage",
            executable_components: &["Hanafubuki.AppImage"],
        }),
        (os, architecture) => Err(format!(
            "The launcher does not support {os}/{architecture}."
        )),
    }
}

pub fn installed_executable(
    core: &ResolvedPath,
    target: &TargetSpec,
) -> Result<Option<ResolvedPath>, String> {
    resolve_existing_components(core, target.executable_components)
}

pub fn resolve_existing_components(
    root: &ResolvedPath,
    components: &[&str],
) -> Result<Option<ResolvedPath>, String> {
    let mut current = root.clone();
    for component in components {
        let child = ResolvedPath::resolve_child(&current, OsStr::new(component))
            .map_err(|error| error.to_string())?;
        current = match ResolvedPath::resolve(child.native()) {
            Ok(path) => path,
            Err(error) if error.is_not_found() => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
    }
    Ok(Some(current))
}

pub fn require_existing(path: &ResolvedPath) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve(path.native()).map_err(|error| match error {
        PathError::NotFound(_) => format!("Expected path is missing: {}", path.native_text()),
        error => error.to_string(),
    })
}

pub fn restore_executable_permissions(
    core: &ResolvedPath,
    target: &TargetSpec,
) -> Result<(), String> {
    let executable = installed_executable(core, target)?
        .ok_or("The repaired Hanafubuki executable is missing.")?;
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(executable.system(), fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Restore executable permissions: {error}"))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}
