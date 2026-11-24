use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_DWORD, REG_SZ, RegCloseKey, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};

pub fn read_machine_dword(
    subkey: &str,
    value_name: &str,
    access: u32,
    label: &str,
) -> Result<Option<u32>, String> {
    read_dword(HKEY_LOCAL_MACHINE, subkey, value_name, access, label)
}

pub fn read_user_dword(
    subkey: &str,
    value_name: &str,
    access: u32,
    label: &str,
) -> Result<Option<u32>, String> {
    read_dword(HKEY_CURRENT_USER, subkey, value_name, access, label)
}

pub fn read_user_string(
    subkey: &str,
    value_name: &str,
    access: u32,
    label: &str,
) -> Result<Option<String>, String> {
    let Some(key) = open_key(HKEY_CURRENT_USER, subkey, access, label, true)? else {
        return Ok(None);
    };
    let value_name_text = value_name;
    let value_name = wide(value_name_text);
    let mut value_type = 0_u32;
    let mut value_size = 0_u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value_name.as_ptr(),
            null(),
            &mut value_type,
            null_mut(),
            &mut value_size,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Read {label} registry value {value_name_text}: OS error {status}"
        ));
    }
    if value_type != REG_SZ || value_size % 2 != 0 {
        return Ok(None);
    }
    let mut value = vec![0_u16; value_size as usize / 2];
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value_name.as_ptr(),
            null(),
            &mut value_type,
            value.as_mut_ptr().cast::<u8>(),
            &mut value_size,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Read {label} registry value {value_name_text}: OS error {status}"
        ));
    }
    while value.last() == Some(&0) {
        value.pop();
    }
    String::from_utf16(&value)
        .map(Some)
        .map_err(|error| format!("Decode {label} registry value {value_name_text}: {error}"))
}

pub fn write_machine_dword(
    subkey: &str,
    value_name: &str,
    value: u32,
    access: u32,
    label: &str,
) -> Result<(), String> {
    let key = open_machine_key(subkey, access, label, false)?
        .ok_or_else(|| format!("Open {label} registry key: path is missing"))?;
    let value_name = wide(value_name);
    let status = unsafe {
        RegSetValueExW(
            key.0,
            value_name.as_ptr(),
            0,
            REG_DWORD,
            (&value as *const u32).cast::<u8>(),
            size_of::<u32>() as u32,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Write {label} registry value: OS error {status}"))
    }
}

fn open_machine_key(
    subkey: &str,
    access: u32,
    label: &str,
    optional: bool,
) -> Result<Option<RegistryKey>, String> {
    open_key(HKEY_LOCAL_MACHINE, subkey, access, label, optional)
}

fn read_dword(
    root: HKEY,
    subkey: &str,
    value_name: &str,
    access: u32,
    label: &str,
) -> Result<Option<u32>, String> {
    let Some(key) = open_key(root, subkey, access, label, true)? else {
        return Ok(None);
    };
    let value_name_text = value_name;
    let value_name = wide(value_name_text);
    let mut value_type = 0_u32;
    let mut value = 0_u32;
    let mut value_size = size_of::<u32>() as u32;
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            value_name.as_ptr(),
            null(),
            &mut value_type,
            (&mut value as *mut u32).cast::<u8>(),
            &mut value_size,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Read {label} registry value {value_name_text}: OS error {status}"
        ));
    }
    if value_type != REG_DWORD || value_size != size_of::<u32>() as u32 {
        return Ok(None);
    }
    Ok(Some(value))
}

fn open_key(
    root: HKEY,
    subkey: &str,
    access: u32,
    label: &str,
    optional: bool,
) -> Result<Option<RegistryKey>, String> {
    let subkey = wide(subkey);
    let mut key: HKEY = null_mut();
    let status = unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, access, &mut key) };
    if optional && (status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND) {
        Ok(None)
    } else if status == ERROR_SUCCESS {
        Ok(Some(RegistryKey(key)))
    } else {
        Err(format!("Open {label} registry key: OS error {status}"))
    }
}

pub(super) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}
