use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use crate::internal::hanafubuki_process::executor::{ExecutorManager, ExecutorStatus};
use serde::{Deserialize, Serialize};

use crate::platform::{self, LauncherInstallKind, LauncherLayout};
use crate::process::{self, ProcessCommand};

const APPLY_ARGUMENT: &str = "--hanafubuki-apply-launcher-update";
const CONFIRM_ARGUMENT: &str = "--hanafubuki-confirm-launcher-update";
const VERSION_ARGUMENT: &str = "--hanafubuki-launcher-version";
const HELPER_NAME: &str = if cfg!(windows) {
    "launcher-update-helper.exe"
} else {
    "launcher-update-helper"
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Transaction {
    nonce: String,
    parent_pid: u32,
    version: String,
    target: String,
    candidate: String,
    install_kind: String,
}

pub fn internal_mode(executor: &ExecutorManager, arguments: &[String]) -> Option<i32> {
    if arguments.len() != 3 {
        return None;
    }
    let value = &arguments[2];
    if arguments[1] == CONFIRM_ARGUMENT {
        return match confirm(value) {
            Ok(()) => None,
            Err(_) => Some(1),
        };
    }
    match arguments[1].as_str() {
        VERSION_ARGUMENT => Some(i32::from(value != env!("CARGO_PKG_VERSION"))),
        APPLY_ARGUMENT => Some(run_helper(executor, value).map_or(1, |()| 0)),
        _ => None,
    }
}

pub fn validate_candidate(
    executor: &ExecutorManager,
    executable: &ResolvedPath,
    version: &str,
) -> Result<(), String> {
    let outcome = ProcessCommand::new(executable.native_text())
        .arg(VERSION_ARGUMENT)
        .arg(version)
        .run(executor, Duration::from_secs(10))
        .map_err(|error| format!("Start launcher version probe: {error}"))?;
    if outcome.status == ExecutorStatus::Exited && outcome.exit_code == Some(0) {
        Ok(())
    } else {
        Err(format!(
            "The downloaded launcher reports an unexpected version: status={:?} exit_code={:?} output={}",
            outcome.status,
            outcome.exit_code,
            outcome.output.trim()
        ))
    }
}

pub fn hand_off(
    executor: &ExecutorManager,
    layout: &LauncherLayout,
    update_root: &ResolvedPath,
    candidate: &ResolvedPath,
    version: &str,
) -> Result<(), String> {
    let nonce = nonce()?;
    let helper = child(update_root, HELPER_NAME)?;
    fs::copy(layout.executable.system(), helper.system())
        .map_err(|error| format!("Prepare launcher update helper: {error}"))?;
    sync_file(&helper)?;
    restore_executable_permissions(&helper)?;
    let helper = platform::require_existing(&helper)?;

    let transaction = Transaction {
        nonce: nonce.clone(),
        parent_pid: std::process::id(),
        version: version.to_string(),
        target: layout.install_target.native_text().to_string(),
        candidate: candidate.native_text().to_string(),
        install_kind: install_kind_name(layout.install_kind).to_string(),
    };
    store_transaction(update_root, &transaction)?;
    ProcessCommand::new(helper.native_text())
        .arg(APPLY_ARGUMENT)
        .arg(&nonce)
        .current_dir(update_root.native())
        .start_independent(executor)
        .map_err(|error| format!("Start launcher update helper: {error}"))?;
    Ok(())
}

fn run_helper(executor: &ExecutorManager, nonce: &str) -> Result<(), String> {
    validate_nonce(nonce)?;
    let helper = ResolvedPath::resolve(
        std::env::current_exe().map_err(|error| format!("Find update helper: {error}"))?,
    )
    .map_err(|error| error.to_string())?;
    let update_root = resolved_parent(&helper)?;
    let transaction = load_transaction(&update_root, nonce)?;
    validate_transaction(&transaction, nonce, &update_root)?;
    wait_for_process_exit(transaction.parent_pid, Duration::from_secs(30))?;

    let target = ResolvedPath::resolve(&transaction.target).map_err(|error| error.to_string())?;
    let candidate =
        ResolvedPath::resolve(&transaction.candidate).map_err(|error| error.to_string())?;
    let install_kind = parse_install_kind(&transaction.install_kind)?;
    let backup = backup_path(&update_root, install_kind)?;
    replace(&target, &candidate, &backup, install_kind)?;

    let executable = installed_executable(&target, install_kind)?;
    let child_process = match ProcessCommand::new(executable.native_text())
        .arg(CONFIRM_ARGUMENT)
        .arg(&transaction.nonce)
        .start_independent(executor)
    {
        Ok(process) => process,
        Err(error) => {
            rollback(&target, &backup, install_kind)?;
            return Err(format!("Start updated launcher: {error}"));
        }
    };

    if wait_for_ack(&update_root, nonce, Duration::from_secs(20)).is_err() {
        if let Err(error) = process::stop(executor, &child_process.executor_id) {
            log::warn!(
                target: "launcher::self_update",
                "stop unconfirmed updated launcher failed executor_id={} error={error}",
                child_process.executor_id
            );
        }
        rollback(&target, &backup, install_kind)?;
        let restored = installed_executable(&target, install_kind)?;
        ProcessCommand::new(restored.native_text())
            .start_independent(executor)
            .map_err(|error| format!("Restart restored launcher: {error}"))?;
        return Err(
            "Updated launcher did not confirm startup; restored the previous version".to_string(),
        );
    }

    remove_path(&backup)?;
    let transaction_path = transaction_path(&update_root, nonce)?;
    let _ = fs::remove_file(transaction_path.system());
    let ack = ack_path(&update_root, nonce)?;
    let _ = fs::remove_file(ack.system());
    for name in ["package.tmp", "candidate"] {
        let path = child(&update_root, name)?;
        let _ = remove_path(&path);
    }
    Ok(())
}

fn confirm(nonce: &str) -> Result<(), String> {
    validate_nonce(nonce)?;
    let layout = platform::launcher_layout()?;
    let state_root = child(&layout.portable_root, ".hanafubuki-launcher")?;
    let state_root = platform::require_existing(&state_root)?;
    let update_root = child(&state_root, "self-update")?;
    let update_root = platform::require_existing(&update_root)?;
    let ack = ack_path(&update_root, nonce)?;
    let mut file = File::create(ack.system())
        .map_err(|error| format!("Create launcher update acknowledgement: {error}"))?;
    file.write_all(nonce.as_bytes())
        .map_err(|error| format!("Write launcher update acknowledgement: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Sync launcher update acknowledgement: {error}"))
}

fn validate_transaction(
    transaction: &Transaction,
    nonce: &str,
    update_root: &ResolvedPath,
) -> Result<(), String> {
    if transaction.nonce != nonce || semver::Version::parse(&transaction.version).is_err() {
        return Err("Launcher update transaction does not match the helper request".to_string());
    }
    let install_kind = parse_install_kind(&transaction.install_kind)?;
    let portable_root = update_root
        .native()
        .parent()
        .and_then(|path| path.parent())
        .ok_or("Launcher update root has an invalid layout")?;
    let portable_root = ResolvedPath::resolve(portable_root).map_err(|error| error.to_string())?;
    let expected_target = match install_kind {
        LauncherInstallKind::File => child(&portable_root, installed_file_name())?,
        LauncherInstallKind::MacApp => child(&portable_root, "Hanafubuki Launcher.app")?,
    };
    let target = ResolvedPath::resolve(&transaction.target).map_err(|error| error.to_string())?;
    if target != platform::require_existing(&expected_target)? {
        return Err("Launcher update target is outside the portable layout".to_string());
    }
    let candidate =
        ResolvedPath::resolve(&transaction.candidate).map_err(|error| error.to_string())?;
    if !candidate.starts_with(update_root) {
        return Err("Launcher update candidate is outside the update workspace".to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn replace(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
    kind: LauncherInstallKind,
) -> Result<(), String> {
    remove_path(backup)?;
    if kind != LauncherInstallKind::File {
        return Err("Windows launcher update received an invalid install kind".to_string());
    }
    replace_windows_file(target, candidate, backup)
}

#[cfg(target_os = "macos")]
fn replace(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
    kind: LauncherInstallKind,
) -> Result<(), String> {
    remove_path(backup)?;
    match kind {
        LauncherInstallKind::MacApp => replace_macos_bundle(target, candidate, backup),
        LauncherInstallKind::File => replace_unix_file(target, candidate, backup),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn replace(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
    kind: LauncherInstallKind,
) -> Result<(), String> {
    remove_path(backup)?;
    if kind != LauncherInstallKind::File {
        return Err("Unix launcher update received an invalid install kind".to_string());
    }
    replace_unix_file(target, candidate, backup)
}

#[cfg(windows)]
fn replace_windows_file(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_ACL_ERRORS, ReplaceFileW};

    let wide = |path: &ResolvedPath| {
        path.system()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let target = wide(target);
    let candidate = wide(candidate);
    let backup = wide(backup);
    let replaced = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            candidate.as_ptr(),
            backup.as_ptr(),
            REPLACEFILE_IGNORE_ACL_ERRORS,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(format!(
            "Install updated launcher: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn replace_unix_file(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
) -> Result<(), String> {
    fs::copy(target.system(), backup.system())
        .map_err(|error| format!("Back up current launcher: {error}"))?;
    File::open(backup.system())
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Sync launcher backup: {error}"))?;
    fs::rename(candidate.system(), target.system())
        .map_err(|error| format!("Install updated launcher: {error}"))
}

#[cfg(target_os = "macos")]
fn replace_macos_bundle(
    target: &ResolvedPath,
    candidate: &ResolvedPath,
    backup: &ResolvedPath,
) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let target_path = CString::new(target.system().as_os_str().as_bytes())
        .map_err(|_| "Launcher target contains an invalid NUL byte")?;
    let candidate_path = CString::new(candidate.system().as_os_str().as_bytes())
        .map_err(|_| "Launcher candidate contains an invalid NUL byte")?;
    let swapped = unsafe {
        libc::renamex_np(
            candidate_path.as_ptr(),
            target_path.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    if swapped != 0 {
        return Err(format!(
            "Install updated launcher bundle: {}",
            std::io::Error::last_os_error()
        ));
    }
    if let Err(error) = fs::rename(candidate.system(), backup.system()) {
        let _ = unsafe {
            libc::renamex_np(
                candidate_path.as_ptr(),
                target_path.as_ptr(),
                libc::RENAME_SWAP,
            )
        };
        return Err(format!("Preserve previous launcher bundle: {error}"));
    }
    Ok(())
}

fn rollback(
    target: &ResolvedPath,
    backup: &ResolvedPath,
    _kind: LauncherInstallKind,
) -> Result<(), String> {
    remove_path(target)?;
    fs::rename(backup.system(), target.system())
        .map_err(|error| format!("Restore previous launcher: {error}"))
}

fn installed_executable(
    target: &ResolvedPath,
    kind: LauncherInstallKind,
) -> Result<ResolvedPath, String> {
    match kind {
        LauncherInstallKind::File => Ok(target.clone()),
        LauncherInstallKind::MacApp => {
            let contents = existing_child(target, "Contents")?;
            let macos = existing_child(&contents, "MacOS")?;
            existing_child(&macos, "hanafubuki-launcher")
        }
    }
}

fn wait_for_ack(root: &ResolvedPath, nonce: &str, timeout: Duration) -> Result<(), String> {
    let ack = ack_path(root, nonce)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if fs::read_to_string(ack.system()).is_ok_and(|content| content == nonce) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("Timed out waiting for launcher startup acknowledgement".to_string())
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_is_alive(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("Timed out waiting for the previous launcher to exit".to_string())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let result = unsafe { WaitForSingleObject(handle, 0) };
    unsafe { CloseHandle(handle) };
    result == WAIT_TIMEOUT
}

fn store_transaction(root: &ResolvedPath, transaction: &Transaction) -> Result<(), String> {
    let path = transaction_path(root, &transaction.nonce)?;
    let temporary = child(root, &format!("transaction-{}.tmp", transaction.nonce))?;
    let bytes = serde_json::to_vec(transaction)
        .map_err(|error| format!("Encode launcher update transaction: {error}"))?;
    let mut file = File::create(temporary.system())
        .map_err(|error| format!("Create launcher update transaction: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("Write launcher update transaction: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Sync launcher update transaction: {error}"))?;
    fs::rename(temporary.system(), path.system())
        .map_err(|error| format!("Publish launcher update transaction: {error}"))
}

fn load_transaction(root: &ResolvedPath, nonce: &str) -> Result<Transaction, String> {
    let path = transaction_path(root, nonce)?;
    let bytes = fs::read(path.system())
        .map_err(|error| format!("Read launcher update transaction: {error}"))?;
    if bytes.len() > 16 * 1024 {
        return Err("Launcher update transaction exceeds the size limit".to_string());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Decode launcher update transaction: {error}"))
}

fn transaction_path(root: &ResolvedPath, nonce: &str) -> Result<ResolvedPath, String> {
    child(root, &format!("transaction-{nonce}.json"))
}

fn ack_path(root: &ResolvedPath, nonce: &str) -> Result<ResolvedPath, String> {
    child(root, &format!("ack-{nonce}"))
}

fn backup_path(root: &ResolvedPath, kind: LauncherInstallKind) -> Result<ResolvedPath, String> {
    child(
        root,
        match kind {
            LauncherInstallKind::File if cfg!(windows) => "previous-launcher.exe",
            LauncherInstallKind::File => "previous-launcher",
            LauncherInstallKind::MacApp => "Previous Hanafubuki Launcher.app",
        },
    )
}

fn nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Generate launcher update nonce: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_nonce(nonce: &str) -> Result<(), String> {
    if nonce.len() == 32 && nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Launcher update nonce is invalid".to_string())
    }
}

fn parse_install_kind(value: &str) -> Result<LauncherInstallKind, String> {
    match value {
        "file" => Ok(LauncherInstallKind::File),
        "macApp" => Ok(LauncherInstallKind::MacApp),
        _ => Err("Launcher update install kind is invalid".to_string()),
    }
}

fn install_kind_name(kind: LauncherInstallKind) -> &'static str {
    match kind {
        LauncherInstallKind::File => "file",
        LauncherInstallKind::MacApp => "macApp",
    }
}

fn installed_file_name() -> &'static str {
    if cfg!(windows) {
        "hanafubuki-launcher.exe"
    } else {
        "hanafubuki-launcher"
    }
}

fn child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    ResolvedPath::resolve_child(parent, OsStr::new(name)).map_err(|error| error.to_string())
}

fn existing_child(parent: &ResolvedPath, name: &str) -> Result<ResolvedPath, String> {
    platform::require_existing(&child(parent, name)?)
}

fn resolved_parent(path: &ResolvedPath) -> Result<ResolvedPath, String> {
    let parent = path
        .native()
        .parent()
        .ok_or("Launcher helper has no parent directory")?;
    ResolvedPath::resolve(parent).map_err(|error| error.to_string())
}

fn sync_file(path: &ResolvedPath) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.write(true);
    options
        .open(path.system())
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Sync launcher update helper: {error}"))
}

fn restore_executable_permissions(_path: &ResolvedPath) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path.system(), fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Set launcher update helper permissions: {error}"))?;
    }
    Ok(())
}

fn remove_path(path: &ResolvedPath) -> Result<(), String> {
    match fs::symlink_metadata(path.system()) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            fs::remove_file(path.system())
                .map_err(|error| format!("Remove {}: {error}", path.native_text()))
        }
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path.system())
            .map_err(|error| format!("Remove {}: {error}", path.native_text())),
        Ok(_) => Err(format!(
            "Refuse to remove unsupported path: {}",
            path.native_text()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Inspect {}: {error}", path.native_text())),
    }
}

#[cfg(test)]
mod tests;
