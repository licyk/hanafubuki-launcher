use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use windows_sys::Win32::System::Registry::{KEY_QUERY_VALUE, KEY_WOW64_64KEY};

use crate::network::NetworkContext;
use crate::platform::windows::{elevation, registry};

const VC_REDIST_URL: &str = "https://aka.ms/vc14/vc_redist.x64.exe";
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;
const USER_AGENT: &str = "Hanafubuki Launcher VC Runtime prerequisite";
const VC_RUNTIME_KEY: &str = r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64";
const VC_RUNTIME_INSTALLED_VALUE: &str = "Installed";
const VC_RUNTIME_INSTALL_ARGUMENTS: &str = "/install /passive /norestart";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VcRuntimeProgress {
    Checking,
    Downloading,
    Download { completed: u64, total: Option<u64> },
    Installing,
}

pub fn ensure_installed(
    network: &NetworkContext,
    mut report: impl FnMut(VcRuntimeProgress),
) -> Result<(), String> {
    report(VcRuntimeProgress::Checking);
    if installed()? {
        log::info!(
            target: "launcher::vc_runtime",
            "Microsoft Visual C++ Redistributable (x64) is already installed"
        );
        return Ok(());
    }

    report(VcRuntimeProgress::Downloading);
    let mut installer = TemporaryInstaller::create()?;
    download_installer(network, &mut installer, &mut report)?;
    installer.sync_and_close()?;

    report(VcRuntimeProgress::Installing);
    let exit_code = install(installer.path())?;
    log::info!(
        target: "launcher::vc_runtime",
        "Microsoft Visual C++ Redistributable installer exited code={exit_code}"
    );

    if !installed()? {
        return Err(
            "The runtime registration is still absent after the installer completed.".to_string(),
        );
    }
    log::info!(
        target: "launcher::vc_runtime",
        "Microsoft Visual C++ Redistributable (x64) installation confirmed"
    );
    Ok(())
}

fn installed() -> Result<bool, String> {
    registry::read_machine_dword(
        VC_RUNTIME_KEY,
        VC_RUNTIME_INSTALLED_VALUE,
        KEY_QUERY_VALUE | KEY_WOW64_64KEY,
        "Microsoft Visual C++ Runtime",
    )
    .map(|value| value == Some(1))
}

fn install(installer: &ResolvedPath) -> Result<u32, String> {
    elevation::run(
        installer.native().as_os_str(),
        OsStr::new(VC_RUNTIME_INSTALL_ARGUMENTS),
        "Start the Microsoft Visual C++ Redistributable installer",
        "Administrator approval for the Microsoft Visual C++ Redistributable installer was cancelled.",
    )
}

fn download_installer(
    network: &NetworkContext,
    installer: &mut TemporaryInstaller,
    report: &mut impl FnMut(VcRuntimeProgress),
) -> Result<(), String> {
    log::info!(
        target: "launcher::vc_runtime",
        "download started url={VC_REDIST_URL} destination={}",
        installer.path().native_text()
    );
    let client = network
        .client(USER_AGENT)
        .map_err(|error| format!("Create Microsoft download client: {error}"))?;
    let mut response = client
        .get(VC_REDIST_URL)
        .send()
        .map_err(|error| format!("Download Microsoft Visual C++ Redistributable: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Download Microsoft Visual C++ Redistributable: {error}"))?;
    let total = response.content_length();
    let completed = copy_bounded(
        &mut response,
        installer.file_mut()?,
        total,
        |completed, total| {
            report(VcRuntimeProgress::Download { completed, total });
        },
    )?;
    log::info!(
        target: "launcher::vc_runtime",
        "download completed bytes={completed} destination={}",
        installer.path().native_text()
    );
    Ok(())
}

fn copy_bounded(
    reader: &mut impl Read,
    writer: &mut impl Write,
    total: Option<u64>,
    mut report: impl FnMut(u64, Option<u64>),
) -> Result<u64, String> {
    if total == Some(0) {
        return Err("The Microsoft Visual C++ Redistributable download is empty.".to_string());
    }
    if total.is_some_and(|total| total > MAX_DOWNLOAD_BYTES) {
        return Err(format!(
            "The Microsoft Visual C++ Redistributable download exceeds {} MiB.",
            MAX_DOWNLOAD_BYTES / (1024 * 1024)
        ));
    }

    let mut buffer = [0_u8; DOWNLOAD_CHUNK_BYTES];
    let mut completed = 0_u64;
    let mut last_reported = 0_u64;
    let mut last_progress_emit = None;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("Read Microsoft Visual C++ Redistributable: {error}"))?;
        if count == 0 {
            break;
        }
        completed = completed.checked_add(count as u64).ok_or_else(|| {
            "The Microsoft Visual C++ Redistributable size overflowed.".to_string()
        })?;
        if completed > MAX_DOWNLOAD_BYTES {
            return Err(format!(
                "The Microsoft Visual C++ Redistributable download exceeds {} MiB.",
                MAX_DOWNLOAD_BYTES / (1024 * 1024)
            ));
        }
        if total.is_some_and(|total| completed > total) {
            return Err(
                "The Microsoft Visual C++ Redistributable download exceeded its declared size."
                    .to_string(),
            );
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| format!("Write Microsoft Visual C++ Redistributable: {error}"))?;
        let now = Instant::now();
        if total == Some(completed)
            || last_progress_emit
                .is_none_or(|last: Instant| now.duration_since(last) >= Duration::from_millis(100))
        {
            report(completed, total);
            last_reported = completed;
            last_progress_emit = Some(now);
        }
    }

    if completed == 0 {
        return Err("The Microsoft Visual C++ Redistributable download is empty.".to_string());
    }
    if total.is_some_and(|total| completed != total) {
        return Err(
            "The Microsoft Visual C++ Redistributable download ended before it was complete."
                .to_string(),
        );
    }
    if last_reported != completed {
        report(completed, total);
    }
    Ok(completed)
}

struct TemporaryInstaller {
    path: ResolvedPath,
    file: Option<File>,
}

impl TemporaryInstaller {
    fn create() -> Result<Self, String> {
        let directory = ResolvedPath::resolve(std::env::temp_dir())
            .map_err(|error| format!("Resolve temporary directory: {error}"))?;
        Self::create_in(&directory)
    }

    fn create_in(directory: &ResolvedPath) -> Result<Self, String> {
        for _ in 0..16 {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random)
                .map_err(|error| format!("Generate temporary installer name: {error}"))?;
            let name = format!("hanafubuki-vc-redist-{}.exe", hex(&random));
            let path = ResolvedPath::resolve_child(directory, OsStr::new(&name))
                .map_err(|error| error.to_string())?;
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.system())
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Create temporary installer: {error}")),
            }
        }
        Err("Create a unique temporary installer: too many name collisions.".to_string())
    }

    fn path(&self) -> &ResolvedPath {
        &self.path
    }

    fn file_mut(&mut self) -> Result<&mut File, String> {
        self.file
            .as_mut()
            .ok_or_else(|| "The temporary installer file is already closed.".to_string())
    }

    fn sync_and_close(&mut self) -> Result<(), String> {
        let Some(file) = self.file.take() else {
            return Err("The temporary installer file is already closed.".to_string());
        };
        file.sync_all()
            .map_err(|error| format!("Flush temporary installer: {error}"))?;
        drop(file);
        Ok(())
    }
}

impl Drop for TemporaryInstaller {
    fn drop(&mut self) {
        self.file.take();
        match fs::remove_file(self.path.system()) {
            Ok(()) => log::debug!(
                target: "launcher::vc_runtime",
                "temporary installer removed path={}",
                self.path.native_text()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => log::warn!(
                target: "launcher::vc_runtime",
                "remove temporary installer failed path={} error={error}",
                self.path.native_text()
            ),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests;
