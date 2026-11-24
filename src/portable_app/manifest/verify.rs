//! Integrity verification of an installed core against its manifest, and
//! the cleanup and repair that bring it back in line.

use super::*;

pub fn verify(core: &ResolvedPath, manifest: &CoreManifest) -> Result<IntegrityStatus, String> {
    validate(manifest)?;
    log::info!(
        target: "launcher::verify",
        "verification started version={} expected_files={} root={}",
        manifest.version,
        manifest.files.len(),
        core.native_text()
    );
    let (expected_files, expected_directories) = expected_entries(manifest)?;
    let mut relative = Vec::new();
    let mut state = VerificationState::default();
    verify_directory(
        core,
        &mut relative,
        &expected_files,
        &expected_directories,
        &mut state,
    )?;
    for expected_path in expected_files.keys() {
        if !state.observed_files.contains(*expected_path) {
            log::warn!(
                target: "launcher::verify",
                "expected file missing or has wrong type path={expected_path}"
            );
        }
    }
    if state.observed_files.len() != expected_files.len() {
        state.repair_required = true;
    }
    let status = if state.repair_required {
        IntegrityStatus::RepairRequired
    } else if state.has_extras {
        IntegrityStatus::ExtrasOnly
    } else {
        IntegrityStatus::Intact
    };
    log::info!(
        target: "launcher::verify",
        "verification completed status={status:?} observed_files={} expected_files={} extras={}",
        state.observed_files.len(),
        expected_files.len(),
        state.has_extras
    );
    Ok(status)
}

pub fn remove_extras(
    core: &ResolvedPath,
    manifest: &CoreManifest,
    mut report: impl FnMut(FileProgress),
) -> Result<(), String> {
    validate(manifest)?;
    log::info!(
        target: "launcher::cleanup",
        "extra-entry cleanup started root={} expected_files={}",
        core.native_text(),
        manifest.files.len()
    );
    let (expected_files, expected_directories) = expected_entries(manifest)?;
    let mut relative = Vec::new();
    let mut removed = 0_u64;
    cleanup_unexpected(
        core,
        &mut relative,
        &expected_files,
        &expected_directories,
        &mut removed,
        &mut report,
    )?;
    log::info!(target: "launcher::cleanup", "extra-entry cleanup completed");
    Ok(())
}

pub fn repair(
    core: &ResolvedPath,
    trusted_root: &ResolvedPath,
    manifest: &CoreManifest,
    mut report: impl FnMut(FileProgress),
) -> Result<(), String> {
    validate(manifest)?;
    log::info!(
        target: "launcher::repair",
        "repair started version={} files={} installed_root={} trusted_root={}",
        manifest.version,
        manifest.files.len(),
        core.native_text(),
        trusted_root.native_text()
    );
    let mut expected_files = Vec::with_capacity(manifest.files.len());
    let mut expected_directories: Vec<(usize, ResolvedPath)> = Vec::new();

    for file in &manifest.files {
        let components = components(&file.path)?;
        let parent_components = &components[..components.len() - 1];
        let parent = compose(core, parent_components)?;
        let target = ResolvedPath::resolve_child(
            &parent,
            OsStr::new(components.last().expect("validated file path")),
        )
        .map_err(|error| error.to_string())?;
        let source = compose(trusted_root, &components)?;
        let source_metadata = fs::symlink_metadata(source.system())
            .map_err(|error| format!("Inspect trusted source {}: {error}", file.path))?;
        if !source_metadata.is_file() || source_metadata.file_type().is_symlink() {
            return Err(format!(
                "Trusted source is not a regular file: {}",
                file.path
            ));
        }

        for depth in 1..=parent_components.len() {
            let directory = compose(core, &parent_components[..depth])?;
            if !expected_directories
                .iter()
                .any(|(_, expected)| expected == &directory)
            {
                expected_directories.push((depth, directory));
            }
        }
        expected_files.push(ExpectedFile {
            target,
            source,
            parent,
            sha256: file.sha256.clone(),
            display_path: file.path.clone(),
        });
    }

    let mut relative = Vec::new();
    let mut removed = 0_u64;
    cleanup_core(
        core,
        &expected_files,
        &expected_directories,
        &mut relative,
        &mut removed,
        &mut report,
    )?;
    expected_directories.sort_by_key(|(depth, _)| *depth);
    for (_, directory) in &expected_directories {
        fs::create_dir_all(directory.system())
            .map_err(|error| format!("Create core directory: {error}"))?;
        log::debug!(
            target: "launcher::repair",
            "required directory ready path={}",
            directory.native_text()
        );
    }

    let mut kept = 0_usize;
    let mut replaced = 0_usize;
    for (index, expected) in expected_files.iter().enumerate() {
        let matches = match fs::symlink_metadata(expected.target.system()) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                hash_file(&expected.target)? == expected.sha256
            }
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(format!(
                    "Inspect core file {}: {error}",
                    expected.display_path
                ));
            }
        };
        if !matches {
            log::info!(
                target: "launcher::repair",
                "replacing file path={}",
                expected.display_path
            );
            replace_file(index, expected)?;
            replaced += 1;
        } else {
            kept += 1;
            log::debug!(
                target: "launcher::repair",
                "keeping matching file path={}",
                expected.display_path
            );
        }
        report(FileProgress {
            completed: (index + 1) as u64,
            total: Some(expected_files.len() as u64),
            current_path: expected.display_path.clone(),
        });
    }
    log::info!(
        target: "launcher::repair",
        "repair completed files={} kept={kept} replaced={replaced}",
        expected_files.len()
    );
    Ok(())
}

struct ExpectedFile {
    target: ResolvedPath,
    source: ResolvedPath,
    parent: ResolvedPath,
    sha256: String,
    display_path: String,
}

#[derive(Default)]
struct VerificationState {
    observed_files: BTreeSet<String>,
    has_extras: bool,
    repair_required: bool,
}

fn expected_entries(
    manifest: &CoreManifest,
) -> Result<(BTreeMap<&str, &str>, BTreeSet<String>), String> {
    let mut expected_files = BTreeMap::new();
    let mut expected_directories = BTreeSet::new();
    for file in &manifest.files {
        let components = components(&file.path)?;
        for depth in 1..components.len() {
            expected_directories.insert(components[..depth].join("/"));
        }
        expected_files.insert(file.path.as_str(), file.sha256.as_str());
    }
    Ok((expected_files, expected_directories))
}

fn verify_directory(
    directory: &ResolvedPath,
    relative: &mut Vec<String>,
    expected_files: &BTreeMap<&str, &str>,
    expected_directories: &BTreeSet<String>,
    state: &mut VerificationState,
) -> Result<(), String> {
    let entries = fs::read_dir(directory.system())
        .map_err(|error| format!("Scan installed core {}: {error}", directory.native_text()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Read installed core entry: {error}"))?;
        let file_name = entry.file_name();
        let child = ResolvedPath::resolve_child(directory, &file_name)
            .map_err(|error| error.to_string())?;
        let Some(name) = file_name.to_str() else {
            state.has_extras = true;
            log::debug!(
                target: "launcher::verify",
                "unexpected non-UTF-8 entry path={}",
                child.native_text()
            );
            continue;
        };
        let metadata = fs::symlink_metadata(child.system())
            .map_err(|error| format!("Inspect installed core entry: {error}"))?;

        relative.push(name.to_string());
        let relative_path = relative.join("/");
        if metadata.file_type().is_symlink() {
            if expected_files.contains_key(relative_path.as_str())
                || expected_directories.contains(&relative_path)
            {
                state.repair_required = true;
                log::warn!(
                    target: "launcher::verify",
                    "expected path is a symbolic link path={relative_path}"
                );
            } else {
                state.has_extras = true;
                log::debug!(
                    target: "launcher::verify",
                    "unexpected symbolic link path={relative_path}"
                );
            }
        } else if metadata.is_dir() {
            if expected_directories.contains(&relative_path) {
                verify_directory(
                    &child,
                    relative,
                    expected_files,
                    expected_directories,
                    state,
                )?;
            } else if expected_files.contains_key(relative_path.as_str()) {
                state.repair_required = true;
                log::warn!(
                    target: "launcher::verify",
                    "expected file is a directory path={relative_path}"
                );
            } else {
                state.has_extras = true;
                log::debug!(
                    target: "launcher::verify",
                    "unexpected directory path={relative_path}"
                );
            }
        } else if metadata.is_file() {
            match expected_files.get(relative_path.as_str()) {
                Some(expected_hash) => {
                    state.observed_files.insert(relative_path.clone());
                    if hash_file(&child)?.as_str() != *expected_hash {
                        state.repair_required = true;
                        log::warn!(
                            target: "launcher::verify",
                            "file hash mismatch path={relative_path}"
                        );
                    } else {
                        log::debug!(
                            target: "launcher::verify",
                            "file verified path={relative_path}"
                        );
                    }
                }
                None if expected_directories.contains(&relative_path) => {
                    state.repair_required = true;
                    log::warn!(
                        target: "launcher::verify",
                        "expected directory is a file path={relative_path}"
                    );
                }
                None => {
                    state.has_extras = true;
                    log::debug!(
                        target: "launcher::verify",
                        "unexpected file path={relative_path}"
                    );
                }
            }
        } else if expected_files.contains_key(relative_path.as_str())
            || expected_directories.contains(&relative_path)
        {
            state.repair_required = true;
            log::warn!(
                target: "launcher::verify",
                "expected path has unsupported type path={relative_path}"
            );
        } else {
            state.has_extras = true;
            log::debug!(
                target: "launcher::verify",
                "unexpected entry with unsupported type path={relative_path}"
            );
        }
        relative.pop();
    }
    Ok(())
}

fn cleanup_unexpected(
    directory: &ResolvedPath,
    relative: &mut Vec<String>,
    expected_files: &BTreeMap<&str, &str>,
    expected_directories: &BTreeSet<String>,
    removed: &mut u64,
    report: &mut impl FnMut(FileProgress),
) -> Result<(), String> {
    let entries = fs::read_dir(directory.system())
        .map_err(|error| format!("Scan installed core {}: {error}", directory.native_text()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Read installed core entry: {error}"))?;
        let file_name = entry.file_name();
        let child = ResolvedPath::resolve_child(directory, &file_name)
            .map_err(|error| error.to_string())?;
        let Some(name) = file_name.to_str() else {
            log::info!(
                target: "launcher::cleanup",
                "removing non-UTF-8 entry path={}",
                child.native_text()
            );
            remove_any(&child)?;
            *removed += 1;
            report_removal(report, *removed, child.native_text());
            continue;
        };

        relative.push(name.to_string());
        let relative_path = relative.join("/");
        let metadata = fs::symlink_metadata(child.system())
            .map_err(|error| format!("Inspect installed core entry: {error}"))?;
        let expected_file = expected_files.contains_key(relative_path.as_str());
        let expected_directory = expected_directories.contains(&relative_path);
        if metadata.file_type().is_symlink() {
            if expected_file || expected_directory {
                return Err(format!(
                    "Installed core changed during cleanup: {relative_path}"
                ));
            }
            log::info!(
                target: "launcher::cleanup",
                "removing unexpected symbolic link path={relative_path}"
            );
            remove_any(&child)?;
            *removed += 1;
            report_removal(report, *removed, &relative_path);
        } else if metadata.is_dir() {
            if expected_directory {
                cleanup_unexpected(
                    &child,
                    relative,
                    expected_files,
                    expected_directories,
                    removed,
                    report,
                )?;
            } else if expected_file {
                return Err(format!(
                    "Installed core changed during cleanup: {relative_path}"
                ));
            } else {
                log::info!(
                    target: "launcher::cleanup",
                    "removing unexpected directory path={relative_path}"
                );
                fs::remove_dir_all(child.system())
                    .map_err(|error| format!("Remove extra core directory: {error}"))?;
                *removed += 1;
                report_removal(report, *removed, &relative_path);
            }
        } else if !expected_file {
            if expected_directory {
                return Err(format!(
                    "Installed core changed during cleanup: {relative_path}"
                ));
            }
            log::info!(
                target: "launcher::cleanup",
                "removing unexpected file path={relative_path}"
            );
            remove_any(&child)?;
            *removed += 1;
            report_removal(report, *removed, &relative_path);
        }
        relative.pop();
    }
    Ok(())
}

fn cleanup_core(
    directory: &ResolvedPath,
    expected_files: &[ExpectedFile],
    expected_directories: &[(usize, ResolvedPath)],
    relative: &mut Vec<String>,
    removed: &mut u64,
    report: &mut impl FnMut(FileProgress),
) -> Result<(), String> {
    let entries = fs::read_dir(directory.system())
        .map_err(|error| format!("Scan installed core {}: {error}", directory.native_text()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Read installed core entry: {error}"))?;
        let file_name = entry.file_name();
        let child = ResolvedPath::resolve_child(directory, &file_name)
            .map_err(|error| error.to_string())?;
        relative.push(file_name.to_string_lossy().into_owned());
        let relative_path = relative.join("/");
        let metadata = fs::symlink_metadata(child.system())
            .map_err(|error| format!("Inspect installed core entry: {error}"))?;
        let expected_file = expected_files
            .iter()
            .any(|expected| expected.target == child);
        let expected_directory = expected_directories
            .iter()
            .any(|(_, expected)| expected == &child);

        if metadata.file_type().is_symlink() {
            log::info!(
                target: "launcher::repair",
                "removing symbolic link path={}",
                child.native_text()
            );
            remove_any(&child)?;
            *removed += 1;
            report_removal(report, *removed, &relative_path);
        } else if metadata.is_dir() {
            if expected_file || !expected_directory {
                log::info!(
                    target: "launcher::repair",
                    "removing directory path={}",
                    child.native_text()
                );
                fs::remove_dir_all(child.system())
                    .map_err(|error| format!("Remove extra core directory: {error}"))?;
                *removed += 1;
                report_removal(report, *removed, &relative_path);
            } else {
                cleanup_core(
                    &child,
                    expected_files,
                    expected_directories,
                    relative,
                    removed,
                    report,
                )?;
            }
        } else if !expected_file {
            log::info!(
                target: "launcher::repair",
                "removing file path={}",
                child.native_text()
            );
            remove_any(&child)?;
            *removed += 1;
            report_removal(report, *removed, &relative_path);
        }
        relative.pop();
    }
    Ok(())
}

fn report_removal(report: &mut impl FnMut(FileProgress), completed: u64, current_path: &str) {
    report(FileProgress {
        completed,
        total: None,
        current_path: current_path.to_string(),
    });
}

fn replace_file(index: usize, expected: &ExpectedFile) -> Result<(), String> {
    let temporary_name = format!("{REPAIR_TEMP_PREFIX}{index}");
    let temporary = ResolvedPath::resolve_child(&expected.parent, OsStr::new(&temporary_name))
        .map_err(|error| error.to_string())?;
    remove_any(&temporary)?;
    let bytes = fs::copy(expected.source.system(), temporary.system())
        .map_err(|error| format!("Copy repaired file {}: {error}", expected.display_path))?;
    log::debug!(
        target: "launcher::repair",
        "file copied to temporary path={} bytes={bytes}",
        expected.display_path
    );
    OpenOptions::new()
        .write(true)
        .open(temporary.system())
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Flush repaired file {}: {error}", expected.display_path))?;
    remove_any(&expected.target)?;
    fs::rename(temporary.system(), expected.target.system())
        .map_err(|error| format!("Install repaired file {}: {error}", expected.display_path))?;
    if hash_file(&expected.target)? != expected.sha256 {
        return Err(format!(
            "Repaired file hash does not match: {}",
            expected.display_path
        ));
    }
    log::info!(
        target: "launcher::repair",
        "file installed path={} bytes={bytes}",
        expected.display_path
    );
    Ok(())
}
