use crate::internal::hanafubuki_webui::instances::InstanceMigrationDirectoryIdentity;
use std::fs;
use std::path::Path;

#[cfg(unix)]
pub(super) fn directory_identity(
    path: &Path,
) -> Result<InstanceMigrationDirectoryIdentity, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "inspect migration directory identity {}: {error}",
            path.display()
        )
    })?;
    crate::internal::hanafubuki_fs_transfer::reject_unsupported_reparse(path, &metadata)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "migration path is not a real directory: {}",
            path.display()
        ));
    }
    Ok(InstanceMigrationDirectoryIdentity {
        storage_id: metadata.dev(),
        file_id: metadata.ino(),
    })
}

#[cfg(windows)]
pub(super) fn directory_identity(
    path: &Path,
) -> Result<InstanceMigrationDirectoryIdentity, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING,
    };

    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "inspect migration directory identity {}: {error}",
            path.display()
        )
    })?;
    crate::internal::hanafubuki_fs_transfer::reject_unsupported_reparse(path, &metadata)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "migration path is not a real directory: {}",
            path.display()
        ));
    }
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "open migration directory identity {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let result = unsafe { GetFileInformationByHandle(handle, &mut information) };
    let information_error = (result == 0).then(std::io::Error::last_os_error);
    let close_result = unsafe { CloseHandle(handle) };
    if let Some(error) = information_error {
        return Err(format!(
            "read migration directory identity {}: {error}",
            path.display(),
        ));
    }
    if close_result == 0 {
        return Err(format!(
            "close migration directory identity {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(InstanceMigrationDirectoryIdentity {
        storage_id: u64::from(information.dwVolumeSerialNumber),
        file_id: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
    })
}

pub(super) fn require_directory_identity(
    label: &str,
    expected: &InstanceMigrationDirectoryIdentity,
    actual: &InstanceMigrationDirectoryIdentity,
) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!("{label} identity changed"))
    }
}
