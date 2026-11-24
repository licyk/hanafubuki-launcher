use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::time::Duration;

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_process::executor::ExecutorManager;
use semver::Version;

use crate::network::NetworkContext;
use crate::platform::{self, LauncherLayout};
use crate::signature::{ReleaseSignature, UpdaterKey};

mod catalog;
mod package;
mod transaction;

const MAX_CATALOG_BYTES: usize = 512 * 1024;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const USER_AGENT: &str = "Hanafubuki Launcher self-update";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfUpdateOutcome {
    Current,
    Restarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfUpdateFileOperation {
    Extracting,
    Updating,
    Removing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelfUpdateFileProgress {
    pub operation: SelfUpdateFileOperation,
    pub completed: u64,
    pub total: Option<u64>,
    pub current_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelfUpdateProgress {
    Preparing,
    Download {
        version: Version,
        completed: u64,
        total: Option<u64>,
    },
    File(SelfUpdateFileProgress),
    Installing,
}

pub fn run_internal_mode(executor: &ExecutorManager) -> Option<i32> {
    transaction::internal_mode(executor, &std::env::args().collect::<Vec<_>>())
}

pub fn check_and_apply(
    executor: &ExecutorManager,
    layout: &LauncherLayout,
    network: &NetworkContext,
    report: impl Fn(SelfUpdateProgress),
) -> SelfUpdateOutcome {
    match try_check_and_apply(executor, layout, network, &report) {
        Ok(outcome) => outcome,
        Err(error) => {
            log::warn!(
                target: "launcher::self_update",
                "self-update check failed; keeping current launcher: {error}"
            );
            SelfUpdateOutcome::Current
        }
    }
}

fn try_check_and_apply(
    executor: &ExecutorManager,
    layout: &LauncherLayout,
    network: &NetworkContext,
    report: &impl Fn(SelfUpdateProgress),
) -> Result<SelfUpdateOutcome, String> {
    let public_key = UpdaterKey::embedded()?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("Decode current launcher version: {error}"))?;
    log::info!(
        target: "launcher::self_update",
        "self-update check started current_version={current}"
    );
    let client = network.client(USER_AGENT)?;
    let fetch_client = client.clone();
    let results = crate::network::fetch_sources(catalog::SOURCES.to_vec(), move |source| {
        log::debug!(
            target: "launcher::self_update",
            "requesting launcher catalog provider={:?}",
            source.provider
        );
        fetch_candidate(&fetch_client, source)
    });
    let mut candidates = Vec::new();
    for (source, result) in catalog::SOURCES.into_iter().zip(results) {
        match result {
            Some(Ok(candidate)) => {
                log::info!(
                    target: "launcher::self_update",
                    "launcher catalog accepted provider={:?} version={}",
                    candidate.provider,
                    candidate.version
                );
                candidates.push(candidate);
            }
            Some(Err(error)) => {
                log::warn!(
                    target: "launcher::self_update",
                    "launcher catalog unavailable provider={:?} error={error}",
                    source.provider
                );
            }
            None => {
                log::warn!(
                    target: "launcher::self_update",
                    "launcher catalog skipped provider={:?}: no answer within {} s of another source",
                    source.provider,
                    crate::network::SOURCE_GRACE.as_secs()
                );
            }
        }
    }
    let Some(candidates) = catalog::reconcile(candidates) else {
        log::warn!(
            target: "launcher::self_update",
            "no mutually trusted launcher update candidate is available"
        );
        return Ok(SelfUpdateOutcome::Current);
    };
    let Some(first) = candidates.first() else {
        log::info!(target: "launcher::self_update", "no launcher update candidate is available");
        return Ok(SelfUpdateOutcome::Current);
    };
    if first.version <= current {
        log::info!(
            target: "launcher::self_update",
            "launcher update not required available_version={} current_version={current}",
            first.version
        );
        return Ok(SelfUpdateOutcome::Current);
    }
    log::info!(
        target: "launcher::self_update",
        "launcher update selected version={} providers={}",
        first.version,
        candidates.len()
    );

    report(SelfUpdateProgress::Preparing);
    let update_root = prepare_update_root(layout, report)?;
    for candidate in candidates {
        match prepare_provider_candidate(
            executor,
            &client,
            layout,
            &update_root,
            &candidate,
            &public_key,
            report,
        ) {
            Ok(prepared) => {
                log::info!(
                    target: "launcher::self_update",
                    "launcher candidate prepared provider={:?} version={} executable={}",
                    candidate.provider,
                    candidate.version,
                    prepared.executable.native_text()
                );
                report(SelfUpdateProgress::Installing);
                report(SelfUpdateProgress::File(SelfUpdateFileProgress {
                    operation: SelfUpdateFileOperation::Updating,
                    completed: 1,
                    total: Some(1),
                    current_path: prepared.install_target.native_text().to_string(),
                }));
                transaction::hand_off(
                    executor,
                    layout,
                    &update_root,
                    &prepared.install_target,
                    &candidate.version.to_string(),
                )?;
                log::info!(
                    target: "launcher::self_update",
                    "launcher replacement handed off version={}",
                    candidate.version
                );
                return Ok(SelfUpdateOutcome::Restarted);
            }
            Err(error) => {
                log::warn!(
                    target: "launcher::self_update",
                    "launcher candidate rejected provider={:?} version={} error={error}",
                    candidate.provider,
                    candidate.version
                );
                clean_provider_workspace(&update_root, report)?;
            }
        }
    }
    Ok(SelfUpdateOutcome::Current)
}

fn fetch_candidate(
    client: &reqwest::blocking::Client,
    source: catalog::Source,
) -> Result<catalog::Candidate, String> {
    let response = client
        .get(source.latest_url)
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(|error| format!("Request launcher latest.json: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Request launcher latest.json: {error}"))?;
    let mut bytes = Vec::new();
    response
        .take((MAX_CATALOG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Read launcher latest.json: {error}"))?;
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("Launcher latest.json exceeds the size limit".to_string());
    }
    catalog::decode(source, &bytes)
}

fn prepare_provider_candidate(
    executor: &ExecutorManager,
    client: &reqwest::blocking::Client,
    layout: &LauncherLayout,
    update_root: &ResolvedPath,
    candidate: &catalog::Candidate,
    public_key: &UpdaterKey,
    report: &impl Fn(SelfUpdateProgress),
) -> Result<package::PreparedCandidate, String> {
    log::info!(
        target: "launcher::self_update",
        "preparing launcher candidate provider={:?} version={} workspace={}",
        candidate.provider,
        candidate.version,
        update_root.native_text()
    );
    clean_provider_workspace(update_root, report)?;
    let package_path = child(update_root, "package.tmp")?;
    download(client, candidate, &package_path, report)?;
    let package_path = platform::require_existing(&package_path)?;
    let bytes = fs::read(package_path.system())
        .map_err(|error| format!("Read downloaded launcher: {error}"))?;
    public_key
        .verify(&bytes, &ReleaseSignature::decode(&candidate.signature)?)
        .map_err(|error| format!("Launcher package: {error}"))?;
    log::info!(
        target: "launcher::self_update",
        "launcher package signature verified provider={:?} version={} bytes={}",
        candidate.provider,
        candidate.version,
        bytes.len()
    );

    let staging = child(update_root, "candidate")?;
    fs::create_dir(staging.system())
        .map_err(|error| format!("Create launcher candidate directory: {error}"))?;
    let staging = platform::require_existing(&staging)?;
    let prepared = package::prepare(&package_path, &staging, layout.install_kind, &|progress| {
        report(SelfUpdateProgress::File(SelfUpdateFileProgress {
            operation: SelfUpdateFileOperation::Extracting,
            completed: progress.completed,
            total: progress.total,
            current_path: progress.current_path,
        }));
    })?;
    transaction::validate_candidate(
        executor,
        &prepared.executable,
        &candidate.version.to_string(),
    )?;
    Ok(prepared)
}

fn download(
    client: &reqwest::blocking::Client,
    candidate: &catalog::Candidate,
    destination: &ResolvedPath,
    report: &impl Fn(SelfUpdateProgress),
) -> Result<(), String> {
    log::info!(
        target: "launcher::self_update",
        "launcher download started version={} destination={}",
        candidate.version,
        destination.native_text()
    );
    let mut response = client
        .get(&candidate.url)
        .timeout(Duration::from_secs(120))
        .send()
        .map_err(|error| format!("Download launcher update: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Download launcher update: {error}"))?;
    let total = response.content_length();
    if total.is_some_and(|size| size > MAX_PACKAGE_BYTES) {
        return Err("Launcher update exceeds the size limit".to_string());
    }
    let mut file = File::create(destination.system())
        .map_err(|error| format!("Create launcher download: {error}"))?;
    let mut completed = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("Read launcher download: {error}"))?;
        if count == 0 {
            break;
        }
        completed = completed
            .checked_add(count as u64)
            .ok_or("Launcher download byte count overflow")?;
        if completed > MAX_PACKAGE_BYTES {
            return Err("Launcher update exceeds the size limit".to_string());
        }
        file.write_all(&buffer[..count])
            .map_err(|error| format!("Write launcher download: {error}"))?;
        report(SelfUpdateProgress::Download {
            version: candidate.version.clone(),
            completed,
            total,
        });
    }
    if completed == 0 || total.is_some_and(|expected| expected != completed) {
        return Err("Launcher update download is incomplete".to_string());
    }
    file.sync_all()
        .map_err(|error| format!("Sync launcher download: {error}"))?;
    log::info!(
        target: "launcher::self_update",
        "launcher download completed version={} bytes={completed} destination={}",
        candidate.version,
        destination.native_text()
    );
    Ok(())
}

fn prepare_update_root(
    layout: &LauncherLayout,
    report: &impl Fn(SelfUpdateProgress),
) -> Result<ResolvedPath, String> {
    let state_root = child(&layout.portable_root, ".hanafubuki-launcher")?;
    reject_symlink(&state_root)?;
    fs::create_dir_all(state_root.system())
        .map_err(|error| format!("Create launcher state directory: {error}"))?;
    let state_root = platform::require_existing(&state_root)?;
    let update_root = child(&state_root, "self-update")?;
    if remove_path(&update_root)? {
        report(SelfUpdateProgress::File(SelfUpdateFileProgress {
            operation: SelfUpdateFileOperation::Removing,
            completed: 1,
            total: None,
            current_path: "self-update".to_string(),
        }));
    }
    fs::create_dir(update_root.system())
        .map_err(|error| format!("Create launcher update directory: {error}"))?;
    let update_root = platform::require_existing(&update_root)?;
    log::info!(
        target: "launcher::self_update",
        "launcher update workspace prepared path={}",
        update_root.native_text()
    );
    Ok(update_root)
}

fn clean_provider_workspace(
    root: &ResolvedPath,
    report: &impl Fn(SelfUpdateProgress),
) -> Result<(), String> {
    let mut removed = 0_u64;
    for name in ["package.tmp", "candidate"] {
        if remove_path(&child(root, name)?)? {
            removed += 1;
            report(SelfUpdateProgress::File(SelfUpdateFileProgress {
                operation: SelfUpdateFileOperation::Removing,
                completed: removed,
                total: None,
                current_path: name.to_string(),
            }));
        }
    }
    Ok(())
}

fn child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve_child(parent, OsStr::new(name)).map_err(|error| error.to_string())
}

fn reject_symlink(path: &ResolvedPath) -> Result<(), String> {
    if fs::symlink_metadata(path.system()).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        Err(format!(
            "Managed launcher path is a symbolic link: {}",
            path.native_text()
        ))
    } else {
        Ok(())
    }
}

fn remove_path(path: &ResolvedPath) -> Result<bool, String> {
    match fs::symlink_metadata(path.system()) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            log::info!(
                target: "launcher::self_update",
                "removing launcher update file path={}",
                path.native_text()
            );
            fs::remove_file(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))?;
            Ok(true)
        }
        Ok(metadata) if metadata.is_dir() => {
            log::info!(
                target: "launcher::self_update",
                "removing launcher update directory path={}",
                path.native_text()
            );
            fs::remove_dir_all(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))?;
            Ok(true)
        }
        Ok(_) => Err(format!(
            "Refuse to remove unsupported path: {}",
            path.native_text()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::debug!(
                target: "launcher::self_update",
                "cleanup skipped missing path={}",
                path.native_text()
            );
            Ok(false)
        }
        Err(error) => Err(format!("Inspect {}: {error}", path.native_text())),
    }
}
