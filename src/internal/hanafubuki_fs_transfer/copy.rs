use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use crate::internal::hanafubuki_fs_transfer::preserve_path_metadata;

/// Copies one regular file without replacing an existing destination.
///
/// Destination handles are closed before metadata is changed. Metadata is
/// then applied by [`preserve_path_metadata`], which keeps the destination
/// writable until its timestamps have been set and restores source
/// permissions last.
pub fn copy_file_create_new(
    source: &Path,
    destination: &Path,
    buffer_size: usize,
    mut checkpoint: impl FnMut() -> Result<(), String>,
    mut on_bytes: impl FnMut(u64) -> Result<(), String>,
) -> Result<(), String> {
    if buffer_size == 0 {
        return Err("filesystem transfer buffer size must be greater than zero".to_owned());
    }

    let mut input = File::open(source)
        .map_err(|error| format!("open transfer source file {}: {error}", source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| {
            format!(
                "create transfer destination file {}: {error}",
                destination.display()
            )
        })?;
    let mut buffer = vec![0_u8; buffer_size];
    loop {
        checkpoint()?;
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("read transfer source file {}: {error}", source.display()))?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).map_err(|error| {
            format!(
                "write transfer destination file {}: {error}",
                destination.display()
            )
        })?;
        on_bytes(read as u64)?;
    }
    // Windows metadata mutation can be rejected while this process still
    // owns an incompatible file handle. Close both handles deliberately before
    // attempting any timestamp or attribute changes.
    drop(output);
    drop(input);
    preserve_path_metadata(source, destination)
}
