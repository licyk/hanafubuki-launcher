use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use flate2::read::GzDecoder;

use crate::platform::{self, LauncherInstallKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageFileProgress {
    pub completed: u64,
    pub total: Option<u64>,
    pub current_path: String,
}

pub struct PreparedCandidate {
    pub install_target: ResolvedPath,
    pub executable: ResolvedPath,
}

pub fn prepare(
    package: &ResolvedPath,
    staging: &ResolvedPath,
    install_kind: LauncherInstallKind,
    report: &impl Fn(PackageFileProgress),
) -> Result<PreparedCandidate, String> {
    match install_kind {
        LauncherInstallKind::File if cfg!(windows) => prepare_windows(package, staging, report),
        LauncherInstallKind::File => prepare_linux(package, staging, report),
        LauncherInstallKind::MacApp => prepare_macos(package, staging, report),
    }
}

fn prepare_windows(
    package: &ResolvedPath,
    staging: &ResolvedPath,
    report: &impl Fn(PackageFileProgress),
) -> Result<PreparedCandidate, String> {
    let file =
        File::open(package.system()).map_err(|error| format!("Open launcher ZIP: {error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Decode launcher ZIP: {error}"))?;
    if archive.len() != 1 {
        return Err("Windows launcher ZIP must contain exactly one file".to_string());
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|error| format!("Read launcher ZIP entry: {error}"))?;
    let entry_name = entry
        .name()
        .map_err(|error| format!("Read launcher ZIP entry name: {error}"))?;
    if entry.is_dir() || entry_name != "hanafubuki-launcher.exe" {
        return Err("Windows launcher ZIP has an unexpected layout".to_string());
    }
    let executable = child(staging, "hanafubuki-launcher.exe")?;
    write_stream(&mut entry, &executable)?;
    report_extraction(report, 1, 1, "hanafubuki-launcher.exe");
    let executable = platform::require_existing(&executable)?;
    Ok(PreparedCandidate {
        install_target: executable.clone(),
        executable,
    })
}

fn prepare_linux(
    package: &ResolvedPath,
    staging: &ResolvedPath,
    report: &impl Fn(PackageFileProgress),
) -> Result<PreparedCandidate, String> {
    let files = extract_tar_files(package, &["hanafubuki-launcher"], report)?;
    let executable = child(staging, "hanafubuki-launcher")?;
    write_bytes(&executable, files["hanafubuki-launcher"].as_slice())?;
    restore_executable_permissions(&executable)?;
    let executable = platform::require_existing(&executable)?;
    Ok(PreparedCandidate {
        install_target: executable.clone(),
        executable,
    })
}

fn prepare_macos(
    package: &ResolvedPath,
    staging: &ResolvedPath,
    report: &impl Fn(PackageFileProgress),
) -> Result<PreparedCandidate, String> {
    let files = extract_tar_files(
        package,
        &[
            "Hanafubuki Launcher.app/Contents/MacOS/hanafubuki-launcher",
            "Hanafubuki Launcher.app/Contents/Info.plist",
            "Hanafubuki Launcher.app/Contents/Resources/icon.icns",
        ],
        report,
    )?;
    let app = create_directory(staging, "Hanafubuki Launcher.app")?;
    let contents = create_directory(&app, "Contents")?;
    let macos = create_directory(&contents, "MacOS")?;
    let resources = create_directory(&contents, "Resources")?;

    let executable = child(&macos, "hanafubuki-launcher")?;
    write_bytes(
        &executable,
        files["Hanafubuki Launcher.app/Contents/MacOS/hanafubuki-launcher"].as_slice(),
    )?;
    restore_executable_permissions(&executable)?;
    let plist = child(&contents, "Info.plist")?;
    write_bytes(
        &plist,
        files["Hanafubuki Launcher.app/Contents/Info.plist"].as_slice(),
    )?;
    let icon = child(&resources, "icon.icns")?;
    write_bytes(
        &icon,
        files["Hanafubuki Launcher.app/Contents/Resources/icon.icns"].as_slice(),
    )?;

    Ok(PreparedCandidate {
        install_target: platform::require_existing(&app)?,
        executable: platform::require_existing(&executable)?,
    })
}

fn extract_tar_files(
    package: &ResolvedPath,
    expected: &[&str],
    report: &impl Fn(PackageFileProgress),
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let file =
        File::open(package.system()).map_err(|error| format!("Open launcher archive: {error}"))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let expected_files: BTreeSet<&str> = expected.iter().copied().collect();
    let allowed_directories = if expected_files
        .iter()
        .any(|path| path.starts_with("Hanafubuki Launcher.app/"))
    {
        BTreeSet::from([
            "Hanafubuki Launcher.app/",
            "Hanafubuki Launcher.app/Contents/",
            "Hanafubuki Launcher.app/Contents/MacOS/",
            "Hanafubuki Launcher.app/Contents/Resources/",
        ])
    } else {
        BTreeSet::new()
    };
    let mut files = std::collections::BTreeMap::new();
    for entry in archive
        .entries()
        .map_err(|error| format!("Read launcher archive: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Read launcher archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("Read launcher archive path: {error}"))?;
        validate_archive_path(&path)?;
        let path = path
            .to_str()
            .ok_or("Launcher archive path is not UTF-8")?
            .to_string();
        if entry.header().entry_type().is_dir() {
            let directory = format!("{}/", path.trim_end_matches('/'));
            if !allowed_directories.contains(directory.as_str()) {
                return Err("Launcher archive contains an unexpected directory".to_string());
            }
            continue;
        }
        if !entry.header().entry_type().is_file() || !expected_files.contains(path.as_str()) {
            return Err("Launcher archive contains an unexpected entry".to_string());
        }
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|error| format!("Read launcher archive file: {error}"))?;
        if content.is_empty() || files.insert(path.clone(), content).is_some() {
            return Err("Launcher archive contains an empty or duplicate file".to_string());
        }
        report_extraction(
            report,
            files.len() as u64,
            expected_files.len() as u64,
            &path,
        );
    }
    if files.len() != expected_files.len() {
        return Err("Launcher archive does not contain the exact file set".to_string());
    }
    Ok(files)
}

fn report_extraction(
    report: &impl Fn(PackageFileProgress),
    completed: u64,
    total: u64,
    current_path: &str,
) {
    report(PackageFileProgress {
        completed,
        total: Some(total),
        current_path: current_path.to_string(),
    });
}

fn validate_archive_path(path: &Path) -> Result<(), String> {
    if path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(())
    } else {
        Err("Launcher archive contains an unsafe path".to_string())
    }
}

fn create_directory(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    let directory = child(parent, name)?;
    fs::create_dir(directory.system())
        .map_err(|error| format!("Create launcher directory: {error}"))?;
    platform::require_existing(&directory)
}

fn child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve_child(parent, OsStr::new(name)).map_err(|error| error.to_string())
}

fn write_stream(reader: &mut impl Read, path: &ResolvedPath) -> Result<(), String> {
    let mut file = File::create(path.system())
        .map_err(|error| format!("Create launcher candidate: {error}"))?;
    std::io::copy(reader, &mut file)
        .map_err(|error| format!("Write launcher candidate: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Sync launcher candidate: {error}"))
}

fn write_bytes(path: &ResolvedPath, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path.system())
        .map_err(|error| format!("Create launcher candidate: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("Write launcher candidate: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Sync launcher candidate: {error}"))
}

fn restore_executable_permissions(_path: &ResolvedPath) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path.system(), fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Set launcher permissions: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
