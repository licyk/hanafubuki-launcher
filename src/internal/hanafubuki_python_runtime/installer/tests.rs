use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use sha2::{Digest, Sha256};

use super::catalog::{PYTHON_DOWNLOADS, find_download};
use super::extract::{
    ArchiveExtractionObserver, COPY_BUFFER_SIZE, copy_with_cancellation, extract_archive,
    extract_archive_with_observer, sanitize_relative_path,
};
use super::*;

struct CancelAfterFirstRead {
    inner: Cursor<Vec<u8>>,
    cancellation: CancellationToken,
    read_once: bool,
}

impl Read for CancelAfterFirstRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        if count > 0 && !self.read_once {
            self.read_once = true;
            self.cancellation.cancel();
        }
        Ok(count)
    }
}

struct CancelAfterEntries {
    remaining: AtomicUsize,
    cancellation: CancellationToken,
}

impl ArchiveExtractionObserver for CancelAfterEntries {
    fn entry_extracted(&self, _path: &Path) {
        if self.remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.cancellation.cancel();
        }
    }
}

fn archive_test_dir(kind: &str) -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    std::env::temp_dir().join(format!(
        "hanafubuki-{kind}-cancel-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn assert_partial_extraction(archive: &Path, target: &Path, cancellation: &CancellationToken) {
    let observer = CancelAfterEntries {
        remaining: AtomicUsize::new(1),
        cancellation: cancellation.clone(),
    };
    let error = extract_archive_with_observer(
        archive,
        target,
        UpwardLinks::Refused,
        cancellation,
        &observer,
    )
    .map_err(classify_install_error)
    .unwrap_err();
    assert_eq!(error, PythonInstallError::Cancelled);
    assert_eq!(std::fs::read(target.join("one.txt")).unwrap(), b"one");
    assert!(!target.join("two.txt").exists());
    assert!(!target.join("three.txt").exists());
}

fn write_raw_tar_entry(
    archive_path: &Path,
    entry_path: &str,
    entry_type: tar::EntryType,
    link_target: Option<&str>,
) {
    let file = File::create(archive_path).unwrap();
    let mut archive = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(0);
    header.set_entry_type(entry_type);
    let bytes = header.as_mut_bytes();
    bytes[..100].fill(0);
    bytes[..entry_path.len()].copy_from_slice(entry_path.as_bytes());
    if let Some(link_target) = link_target {
        bytes[157..257].fill(0);
        bytes[157..157 + link_target.len()].copy_from_slice(link_target.as_bytes());
    }
    header.set_cksum();
    archive.append(&header, io::empty()).unwrap();
    archive.finish().unwrap();
}

#[test]
fn python_runtime_manager_controlled_stream_cancellation_preserves_partial_download_bytes() {
    let cancellation = CancellationToken::default();
    let mut reader = CancelAfterFirstRead {
        inner: Cursor::new(vec![7_u8; COPY_BUFFER_SIZE * 2]),
        cancellation: cancellation.clone(),
        read_once: false,
    };
    let mut output = Vec::new();
    let error = copy_with_cancellation(&mut reader, &mut output, &cancellation).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(output.len(), COPY_BUFFER_SIZE);
    assert!(output.iter().all(|byte| *byte == 7));
}

#[test]
fn python_runtime_manager_cancellation_is_checked_before_streaming_or_extraction() {
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let mut output = Vec::new();
    let error =
        copy_with_cancellation(&mut Cursor::new(vec![1_u8; 32]), &mut output, &cancellation)
            .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(output.is_empty());

    let target = std::env::temp_dir().join(format!(
        "hanafubuki-cancelled-extract-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("partial"), b"keep").unwrap();
    let error = extract_archive(
        Path::new("missing.zip"),
        &target,
        UpwardLinks::Refused,
        &cancellation,
    )
    .unwrap_err();
    assert_eq!(error, CANCELLED_MARKER);
    assert_eq!(std::fs::read(target.join("partial")).unwrap(), b"keep");
    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn python_runtime_manager_zip_cancels_between_real_archive_entries() {
    let base = archive_test_dir("zip");
    let archive_path = base.join("test.zip");
    let target = base.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let archive_file = File::create(&archive_path).unwrap();
    let mut archive = zip::ZipWriter::new(archive_file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in [
        ("one.txt", b"one".as_slice()),
        ("two.txt", b"two".as_slice()),
        ("three.txt", b"three".as_slice()),
    ] {
        archive.start_file(name, options).unwrap();
        archive.write_all(data).unwrap();
    }
    archive.finish().unwrap();

    let cancellation = CancellationToken::default();
    assert_partial_extraction(&archive_path, &target, &cancellation);
    assert!(sanitize_relative_path(Path::new("../escape")).is_err());
    assert!(!base.join("escape").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn python_runtime_manager_tar_cancels_between_real_archive_entries() {
    let base = archive_test_dir("tar");
    let archive_path = base.join("test.tar");
    let target = base.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let archive_file = File::create(&archive_path).unwrap();
    let mut archive = tar::Builder::new(archive_file);
    for (name, data) in [
        ("one.txt", b"one".as_slice()),
        ("two.txt", b"two".as_slice()),
        ("three.txt", b"three".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append(&header, data).unwrap();
    }
    archive.finish().unwrap();

    let cancellation = CancellationToken::default();
    assert_partial_extraction(&archive_path, &target, &cancellation);
    assert!(sanitize_relative_path(Path::new("../../escape")).is_err());
    assert!(!base.join("escape").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn python_runtime_manager_rejects_real_zip_parent_traversal() {
    let base = archive_test_dir("zip-traversal");
    let archive_path = base.join("malicious.zip");
    let target = base.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let file = File::create(&archive_path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("../zip-escape", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"escape").unwrap();
    archive.finish().unwrap();

    let error = extract_archive(
        &archive_path,
        &target,
        UpwardLinks::Refused,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("unsafe zip entry path"), "{error}");
    assert!(!base.join("zip-escape").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn python_runtime_manager_rejects_real_tar_paths_and_external_links() {
    let base = archive_test_dir("tar-traversal");
    let target = base.join("target");
    std::fs::create_dir_all(&target).unwrap();

    let parent_archive = base.join("parent.tar");
    write_raw_tar_entry(
        &parent_archive,
        "../parent-escape",
        tar::EntryType::Regular,
        None,
    );
    let error = extract_archive(
        &parent_archive,
        &target,
        UpwardLinks::Refused,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("unsafe archive path"), "{error}");
    assert!(!base.join("parent-escape").exists());

    let absolute_escape = base.join("absolute-escape");
    let absolute_archive = base.join("absolute.tar");
    write_raw_tar_entry(
        &absolute_archive,
        absolute_escape.to_str().unwrap(),
        tar::EntryType::Regular,
        None,
    );
    let error = extract_archive(
        &absolute_archive,
        &target,
        UpwardLinks::Refused,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("unsafe archive path"), "{error}");
    assert!(!absolute_escape.exists());

    for (name, entry_type, link_target, outside) in [
        (
            "symlink.tar",
            tar::EntryType::Symlink,
            "../../symlink-escape",
            base.join("symlink-escape"),
        ),
        (
            "hard-link.tar",
            tar::EntryType::Link,
            "../hard-link-escape",
            base.join("hard-link-escape"),
        ),
    ] {
        let archive_path = base.join(name);
        write_raw_tar_entry(&archive_path, "links/out", entry_type, Some(link_target));
        let error = extract_archive(
            &archive_path,
            &target,
            UpwardLinks::Refused,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert!(error.contains("unsafe tar link target"), "{error}");
        assert!(!outside.exists());
        assert!(!target.join("links/out").exists());
    }
    let _ = std::fs::remove_dir_all(base);
}

/// A tar of the file `top/lib/x` and the symbolic links `(path, target)`.
fn tar_with_links(links: &[(&str, &str)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(1);
    header.set_mode(0o644);
    builder
        .append_data(&mut header, "top/lib/x", &b"x"[..])
        .unwrap();
    for (path, target) in links {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder.append_link(&mut header, path, target).unwrap();
    }
    builder.into_inner().unwrap()
}

#[cfg(unix)]
#[test]
fn a_link_points_up_only_through_real_directories_and_within_its_floor() {
    let base = archive_test_dir("tar-links");
    std::fs::create_dir_all(&base).unwrap();
    let extract = |links: &[(&str, &str)], upward_links: UpwardLinks| {
        let archive = base.join("links.tar");
        std::fs::write(&archive, tar_with_links(links)).unwrap();
        let target = base.join(format!("target-{}", links.len()));
        let _ = std::fs::remove_dir_all(&target);
        extract_archive(
            &archive,
            &target,
            upward_links,
            &CancellationToken::default(),
        )
        .map(|()| target)
    };

    let target = extract(&[("top/bin/x", "../lib/x")], UpwardLinks::Within(1)).unwrap();
    assert_eq!(std::fs::read(target.join("top/bin/x")).unwrap(), b"x");
    // Out of `top`, which a floor of 1 keeps it in, though not out of the
    // target; a `..` after a name.
    for (path, link) in [("top/bin/x", "../../x"), ("top/bin/x", "../lib/../../x")] {
        let error = extract(&[(path, link)], UpwardLinks::Within(1)).unwrap_err();
        assert!(error.contains("unsafe tar link target"), "{error}");
    }
    assert!(extract(&[("top/bin/x", "../../x")], UpwardLinks::Within(0)).is_ok());
    // `top/a/b/d` is `top/a`, so `..` twice from it is the target, not `top`.
    let error = extract(
        &[("top/a/b/d", "../../a"), ("top/a/b/d/l", "../../lib/x")],
        UpwardLinks::Within(1),
    )
    .unwrap_err();
    assert!(error.contains("unsafe tar link target"), "{error}");
    // Refused, no link points up at all; one below its directory is kept.
    let error = extract(&[("top/bin/x", "../lib/x")], UpwardLinks::Refused).unwrap_err();
    assert!(error.contains("unsafe tar link target"), "{error}");
    let target = extract(&[("top/y", "lib/x")], UpwardLinks::Refused).unwrap();
    assert_eq!(std::fs::read(target.join("top/y")).unwrap(), b"x");
    assert!(error.contains("unsafe tar link target"), "{error}");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn download_table_offers_every_version_on_each_amd64_platform() {
    for item in PYTHON_DOWNLOADS {
        assert!(
            PythonVersion::try_from(item.version).is_ok(),
            "{} names an unsupported version",
            item.name
        );
    }
    for version in PythonVersion::ALL {
        for platform in ["linux", "macos", "windows"] {
            assert!(
                find_download(version, platform, "amd64").is_ok(),
                "no {version} download for {platform}/amd64"
            );
        }
    }
}

#[test]
fn every_download_pins_a_distinct_lowercase_sha256() {
    let mut seen = HashSet::new();
    for item in PYTHON_DOWNLOADS {
        assert_eq!(item.sha256.len(), 64, "{} digest length", item.name);
        assert!(
            item.sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "{} digest is not lowercase hex",
            item.name
        );
        assert!(
            seen.insert(item.sha256),
            "{} repeats another entry's digest",
            item.name
        );
        assert!(!item.urls.is_empty(), "{} has no URL", item.name);
    }
}

/// Serves `body` over plain HTTP to `requests` connections, one at a time.
fn serve_archive(body: Vec<u8>, requests: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (format!("http://{address}"), handle)
}

fn python_zip() -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file(
            "python/bin/python3",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(b"interpreter").unwrap();
    archive.finish().unwrap().into_inner()
}

fn local_download(base: &str, mirrors: usize, sha256: String) -> PythonDownload {
    let urls = (0..mirrors)
        .map(|index| &*format!("{base}/mirror-{index}/python.zip").leak())
        .collect::<Vec<_>>();
    PythonDownload {
        name: "cpython-test-install_only.zip",
        version: "3.12",
        platform: "linux",
        arch: "amd64",
        sha256: sha256.leak(),
        urls: urls.leak(),
    }
}

fn local_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
}

#[test]
fn mismatching_digest_is_rejected_on_every_mirror_and_installs_nothing() {
    let base = archive_test_dir("digest-mismatch");
    let target = base.join("python");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("previous-install"), b"keep").unwrap();
    let (server, handle) = serve_archive(python_zip(), 2);
    let source = local_download(&server, 2, "0".repeat(64));

    let error = install_from_source(
        InstallStandalonePythonRequest {
            version: PythonVersion::try_from("3.12").unwrap(),
            target_dir: target.clone(),
            overwrite: Some(true),
        },
        source,
        &local_client(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    handle.join().unwrap();

    assert_eq!(error.matches("SHA-256 mismatch").count(), 2, "{error}");
    assert!(error.contains(&"0".repeat(64)), "{error}");
    let remaining = std::fs::read_dir(&target)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(remaining, ["previous-install"]);
    assert_eq!(
        std::fs::read(target.join("previous-install")).unwrap(),
        b"keep"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn matching_digest_installs_the_archive() {
    let base = archive_test_dir("digest-match");
    let target = base.join("python");
    let body = python_zip();
    let digest = hex::encode(Sha256::digest(&body));
    let (server, handle) = serve_archive(body, 1);

    let result = install_from_source(
        InstallStandalonePythonRequest {
            version: PythonVersion::try_from("3.12").unwrap(),
            target_dir: target.clone(),
            overwrite: None,
        },
        local_download(&server, 1, digest),
        &local_client(),
        &CancellationToken::default(),
    )
    .unwrap();
    handle.join().unwrap();

    assert_eq!(result.source_url, format!("{server}/mirror-0/python.zip"));
    assert_eq!(
        std::fs::read(target.join("bin/python3")).unwrap(),
        b"interpreter"
    );
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_installer_refuses_a_link_that_points_up() {
    let base = archive_test_dir("installer-link");
    let target = base.join("python");
    let body = tar_with_links(&[("top/bin/x", "../lib/x")]);
    let digest = hex::encode(Sha256::digest(&body));
    let (server, handle) = serve_archive(body, 1);
    let source = PythonDownload {
        name: "cpython-test-install_only.tar",
        ..local_download(&server, 1, digest)
    };

    let error = install_from_source(
        InstallStandalonePythonRequest {
            version: PythonVersion::try_from("3.12").unwrap(),
            target_dir: target.clone(),
            overwrite: None,
        },
        source,
        &local_client(),
        &CancellationToken::default(),
    )
    .unwrap_err();
    handle.join().unwrap();

    assert!(error.contains("unsafe tar link target"), "{error}");
    assert!(!target.join("top/bin/x").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_bounded_fetch_reads_a_small_body_and_refuses_a_larger_one() {
    let body = b"{\"version\":\"1.0.0\"}".to_vec();
    let (base, server) = serve_archive(body.clone(), 2);
    let url = format!("{base}/latest.json");
    let timeout = std::time::Duration::from_secs(10);

    assert_eq!(fetch_bounded(&url, 64, timeout, None).unwrap(), body);
    let refused = fetch_bounded(&url, 8, timeout, None).unwrap_err();
    assert!(refused.contains("larger than 8 bytes"), "{refused}");
    server.join().unwrap();
}
