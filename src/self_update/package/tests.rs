use std::io::Write;
use std::sync::Mutex;

use tempfile::tempdir;

use super::*;

fn resolved_directory(path: &Path) -> ResolvedPath {
    fs::create_dir_all(path).unwrap();
    ResolvedPath::resolve(path).unwrap()
}

fn ignore_progress(_: PackageFileProgress) {}

#[test]
fn extracts_the_exact_linux_launcher() {
    let temp = tempdir().unwrap();
    let root = resolved_directory(temp.path());
    let package = child(&root, "launcher.tar.gz").unwrap();
    let file = File::create(package.system()).unwrap();
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        file,
        flate2::Compression::default(),
    ));
    let content = b"launcher";
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder
        .append_data(&mut header, "hanafubuki-launcher", content.as_slice())
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    let package = ResolvedPath::resolve(package.native()).unwrap();
    let staging = resolved_directory(&temp.path().join("staging"));

    let progress = Mutex::new(Vec::new());
    let candidate = prepare_linux(&package, &staging, &|entry| {
        progress.lock().unwrap().push(entry);
    })
    .unwrap();
    assert_eq!(fs::read(candidate.executable.system()).unwrap(), content);
    let progress = progress.into_inner().unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].completed, 1);
    assert_eq!(progress[0].total, Some(1));
    assert_eq!(progress[0].current_path, "hanafubuki-launcher");
}

#[test]
fn rejects_tar_path_traversal() {
    let temp = tempdir().unwrap();
    let root = resolved_directory(temp.path());
    let package = child(&root, "launcher.tar.gz").unwrap();
    let file = File::create(package.system()).unwrap();
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    // A minimal raw tar header is used because tar::Builder correctly rejects unsafe paths.
    let mut header = [0u8; 512];
    let unsafe_name = b"../escaped-launcher";
    header[..unsafe_name.len()].copy_from_slice(unsafe_name);
    header[100..108].copy_from_slice(b"0000755\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124..136].copy_from_slice(b"00000000000\0");
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    encoder.write_all(&header).unwrap();
    encoder.write_all(&[0u8; 1024]).unwrap();
    encoder.finish().unwrap();
    let package = ResolvedPath::resolve(package.native()).unwrap();

    assert!(extract_tar_files(&package, &["hanafubuki-launcher"], &ignore_progress).is_err());
}

#[test]
fn extracts_the_exact_windows_launcher_zip() {
    let temp = tempdir().unwrap();
    let root = resolved_directory(temp.path());
    let package = child(&root, "launcher.zip").unwrap();
    let file = File::create(package.system()).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(
            "hanafubuki-launcher.exe",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"windows-launcher").unwrap();
    archive.finish().unwrap();
    let package = ResolvedPath::resolve(package.native()).unwrap();
    let staging = resolved_directory(&temp.path().join("windows-staging"));

    let candidate = prepare_windows(&package, &staging, &ignore_progress).unwrap();
    assert_eq!(
        fs::read(candidate.executable.system()).unwrap(),
        b"windows-launcher"
    );
}

#[test]
fn extracts_the_exact_macos_application_bundle() {
    let temp = tempdir().unwrap();
    let root = resolved_directory(temp.path());
    let package = child(&root, "launcher-macos.tar.gz").unwrap();
    let file = File::create(package.system()).unwrap();
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        file,
        flate2::Compression::default(),
    ));
    for (path, content, mode) in [
        (
            "Hanafubuki Launcher.app/Contents/MacOS/hanafubuki-launcher",
            b"mac-launcher".as_slice(),
            0o755,
        ),
        (
            "Hanafubuki Launcher.app/Contents/Info.plist",
            b"plist".as_slice(),
            0o644,
        ),
        (
            "Hanafubuki Launcher.app/Contents/Resources/icon.icns",
            b"icon".as_slice(),
            0o644,
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(mode);
        header.set_cksum();
        builder.append_data(&mut header, path, content).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
    let package = ResolvedPath::resolve(package.native()).unwrap();
    let staging = resolved_directory(&temp.path().join("macos-staging"));

    let candidate = prepare_macos(&package, &staging, &ignore_progress).unwrap();
    assert_eq!(
        fs::read(candidate.executable.system()).unwrap(),
        b"mac-launcher"
    );
    assert_eq!(
        candidate.install_target.file_name(),
        Some(OsStr::new("Hanafubuki Launcher.app"))
    );
}
