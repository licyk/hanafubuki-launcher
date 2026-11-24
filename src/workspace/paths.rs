use std::ffi::OsStr;
use std::fs;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;

use crate::platform::{self, LauncherLayout};

pub struct WorkspacePaths {
    pub launcher_directory: ResolvedPath,
    pub settings: ResolvedPath,
    pub manifest: ResolvedPath,
    pub data: ResolvedPath,
    pub core: ResolvedPath,
    pub staging: ResolvedPath,
}

pub fn prepare(layout: &LauncherLayout) -> Result<WorkspacePaths, String> {
    let launcher_directory = layout.portable_root.clone();
    let root = child(&launcher_directory, ".hanafubuki-launcher")?;
    fs::create_dir_all(root.system())
        .map_err(|error| format!("Create launcher directory: {error}"))?;
    let root = platform::require_existing(&root)?;

    let settings = child(&root, "settings.json")?;
    let manifest = child(&root, "core-manifest.json")?;
    let data = create_child_directory(&root, "data")?;
    let core = create_child_directory(&root, "core")?;
    let staging = child(&root, "staging")?;
    log::info!(
        target: "launcher::paths",
        "workspace prepared launcher_directory={} state_root={} data={} core={} staging={}",
        launcher_directory.native_text(),
        root.native_text(),
        data.native_text(),
        core.native_text(),
        staging.native_text()
    );
    Ok(WorkspacePaths {
        launcher_directory,
        settings,
        manifest,
        data,
        core,
        staging,
    })
}

pub fn initialize_logging(layout: &LauncherLayout) -> Result<(), String> {
    let root = child(&layout.portable_root, ".hanafubuki-launcher")?;
    fs::create_dir_all(root.system())
        .map_err(|error| format!("Create launcher directory: {error}"))?;
    let root = platform::require_existing(&root)?;
    let logs = create_child_directory(&root, "logs")?;
    crate::logger::initialize(&logs)?;
    Ok(())
}

fn create_child_directory(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    let path = child(parent, name)?;
    if fs::symlink_metadata(path.system()).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "Managed launcher directory is a symbolic link: {name}"
        ));
    }
    fs::create_dir_all(path.system()).map_err(|error| format!("Create {name}: {error}"))?;
    platform::require_existing(&path)
}

fn child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve_child(parent, OsStr::new(name)).map_err(|error| error.to_string())
}
