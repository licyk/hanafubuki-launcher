use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use sha2::{Digest, Sha256};

use super::files;
use super::{ComponentPath, DownloadTarget, ProcessEnvironment, StepEnvironment};
use crate::internal::hanafubuki_webui::install::InstanceInstallStageError;

/// A loopback HTTP server answering every request with `body` (status 200),
/// counting the requests it served.
struct Server {
    url: String,
    served: Arc<AtomicUsize>,
}

fn serve(body: Vec<u8>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let served = Arc::new(AtomicUsize::new(0));
    let count = served.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            count.fetch_add(1, Ordering::SeqCst);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    Server { url, served }
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn component(root: &Path, relative: &str) -> ComponentPath {
    ComponentPath {
        external: false,
        component: "core".to_string(),
        root: root.to_path_buf(),
        relative: PathBuf::from(relative),
    }
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in entries {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn failure(result: Result<(), InstanceInstallStageError>) -> String {
    match result {
        Err(InstanceInstallStageError::Failed(error)) => error,
        other => panic!("the step did not fail: {other:?}"),
    }
}

#[test]
fn a_download_is_kept_only_when_its_digest_matches() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    let body = b"the release".to_vec();
    let server = serve(body.clone());
    let url = format!("{}/release.bin", server.url);
    let target = DownloadTarget::File(component(&core, "bin/release.bin"));
    let environment = StepEnvironment::default();
    let cancellation = CancellationToken::default();

    files::download(&url, &sha256(&body), &target, &environment, &cancellation).unwrap();
    assert_eq!(std::fs::read(core.join("bin/release.bin")).unwrap(), body);

    let error = failure(files::download(
        &url,
        &"0".repeat(64),
        &target,
        &environment,
        &cancellation,
    ));
    assert!(error.contains("SHA-256 mismatch"), "{error}");
    // The file it would have replaced is as it was, and nothing is left beside it.
    assert_eq!(std::fs::read(core.join("bin/release.bin")).unwrap(), body);
    assert_eq!(std::fs::read_dir(core.join("bin")).unwrap().count(), 1);
}

#[test]
fn a_github_download_tries_the_mirror_first() {
    let environment = StepEnvironment {
        github_mirror: Some("https://mirror.example/https://github.com/".to_string()),
        ..StepEnvironment::default()
    };
    assert_eq!(
        environment.download_urls("https://github.com/o/r/releases/a.zip"),
        [
            "https://mirror.example/https://github.com/o/r/releases/a.zip",
            "https://github.com/o/r/releases/a.zip"
        ]
    );
    assert_eq!(
        environment.download_urls("https://example.com/a.zip"),
        ["https://example.com/a.zip"]
    );
}

#[test]
fn an_archive_is_unpacked_beside_its_target_and_moved_into_place() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    let archive = zip_of(&[
        ("server", b"#!/bin/sh\n"),
        ("site/index.html", b"<h1>v1</h1>"),
    ]);
    let server = serve(archive.clone());
    let target = DownloadTarget::UnpackInto {
        into: component(&core, ""),
        strip: 0,
        replace: false,
    };
    let cancellation = CancellationToken::default();
    files::download(
        &format!("{}/site.zip", server.url),
        &sha256(&archive),
        &target,
        &StepEnvironment::default(),
        &cancellation,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(core.join("site/index.html")).unwrap(),
        b"<h1>v1</h1>"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(core.join("server"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }
    // A second version replaces what it ships and keeps what it does not.
    std::fs::write(core.join("user-file"), b"mine").unwrap();
    let second = zip_of(&[("site/index.html", b"<h1>v2</h1>")]);
    let archive_path = root.path().join("second.zip");
    std::fs::write(&archive_path, &second).unwrap();
    files::unpack(
        &archive_path,
        &component(&core, ""),
        0,
        false,
        &cancellation,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(core.join("site/index.html")).unwrap(),
        b"<h1>v2</h1>"
    );
    assert_eq!(std::fs::read(core.join("user-file")).unwrap(), b"mine");
    let staged: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".unpack-"))
        .collect();
    assert!(staged.is_empty());
}

#[test]
fn an_archive_entry_that_would_leave_its_component_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    let archive_path = root.path().join("evil.zip");
    std::fs::write(&archive_path, zip_of(&[("../outside.txt", b"escaped")])).unwrap();
    let error = failure(files::unpack(
        &archive_path,
        &component(&core, ""),
        0,
        false,
        &CancellationToken::default(),
    ));
    assert!(error.contains("unsafe zip entry path"), "{error}");
    assert!(!root.path().join("outside.txt").exists());
    assert!(!core.exists());

    #[cfg(unix)]
    {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, "link", "../../etc")
            .unwrap();
        let tarball = builder.into_inner().unwrap().finish().unwrap();
        let tar_path = root.path().join("evil.tar.gz");
        std::fs::write(&tar_path, tarball).unwrap();
        let error = failure(files::unpack(
            &tar_path,
            &component(&core, ""),
            0,
            false,
            &CancellationToken::default(),
        ));
        assert!(error.contains("unsafe tar link target"), "{error}");
        assert!(!core.exists());
    }
}

#[test]
fn a_written_file_stays_inside_its_component() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    files::write_file(&component(&core, "conf/app.toml"), "port = 1").unwrap();
    assert_eq!(
        std::fs::read_to_string(core.join("conf/app.toml")).unwrap(),
        "port = 1"
    );

    #[cfg(unix)]
    {
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, core.join("linked")).unwrap();
        let error = failure(files::write_file(
            &component(&core, "linked/escaped.txt"),
            "x",
        ));
        assert!(
            error.contains("resolves outside the `core` component"),
            "{error}"
        );
        assert!(!outside.join("escaped.txt").exists());
    }
}

#[test]
fn a_probe_waits_for_an_answer_on_the_loopback_interface() {
    let server = serve(b"ok".to_vec());
    let cancellation = CancellationToken::default();
    files::probe(
        &format!("{}/health", server.url),
        Duration::from_secs(5),
        &cancellation,
    )
    .unwrap();
    assert_eq!(server.served.load(Ordering::SeqCst), 1);

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    let error = failure(files::probe(
        &format!("http://{address}/"),
        Duration::from_millis(300),
        &cancellation,
    ));
    assert!(error.contains("was not answered within"), "{error}");
    let error = failure(files::probe(
        "http://example.com/",
        Duration::from_secs(1),
        &cancellation,
    ));
    assert!(error.contains("not on the loopback interface"), "{error}");
}

#[test]
fn strip_drops_the_leading_components_of_every_entry() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    let archive = zip_of(&[
        ("node-v24.11.0-linux-x64/bin/node", b"#!/bin/sh\n"),
        ("node-v24.11.0-linux-x64/README.md", b"node"),
    ]);
    let server = serve(archive.clone());
    let target = DownloadTarget::UnpackInto {
        into: component(&node, ""),
        strip: 1,
        replace: false,
    };
    let cancellation = CancellationToken::default();
    files::download(
        &format!("{}/node.zip", server.url),
        &sha256(&archive),
        &target,
        &StepEnvironment::default(),
        &cancellation,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(node.join("bin/node")).unwrap(),
        b"#!/bin/sh\n"
    );
    assert_eq!(std::fs::read(node.join("README.md")).unwrap(), b"node");
    assert!(!node.join("node-v24.11.0-linux-x64").exists());

    // Two top directories whose entries do not collide merge, and a link
    // inside the kept tree stays a link to its sibling.
    #[cfg(unix)]
    {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, content) in [("one/a/x.txt", "x"), ("two/b/y.txt", "y")] {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, name, content.as_bytes())
                .unwrap();
        }
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, "one/a/link", "x.txt")
            .unwrap();
        let tar_path = root.path().join("two.tar");
        std::fs::write(&tar_path, builder.into_inner().unwrap()).unwrap();
        let into = root.path().join("merged");
        files::unpack(&tar_path, &component(&into, ""), 1, false, &cancellation).unwrap();
        assert_eq!(std::fs::read(into.join("a/link")).unwrap(), b"x");
        assert!(
            std::fs::symlink_metadata(into.join("a/link"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(std::fs::read(into.join("b/y.txt")).unwrap(), b"y");
    }
    let staged = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".unpack-"))
        .count();
    assert_eq!(staged, 0);
}

#[test]
fn strip_refuses_an_entry_with_too_few_components_and_entries_that_collide() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    let unpack = |entries: &[(&str, &[u8])]| {
        let archive_path = root.path().join("archive.zip");
        std::fs::write(&archive_path, zip_of(entries)).unwrap();
        failure(files::unpack(
            &archive_path,
            &component(&core, ""),
            1,
            false,
            &CancellationToken::default(),
        ))
    };
    let error = unpack(&[("top/bin/tool", b"tool"), ("LICENSE", b"text")]);
    assert!(
        error.contains("archive entry `LICENSE` has nothing left once 1 leading path"),
        "{error}"
    );
    let error = unpack(&[("one/tool", b"1"), ("two/tool", b"2")]);
    assert!(
        error.contains("two archive entries strip to `tool`"),
        "{error}"
    );
    assert!(!core.exists());
}

/// Node.js's own archives link `bin/npm` to `../lib/...`: a link may point
/// up, but not out of the directory `strip` keeps.
#[cfg(unix)]
#[test]
fn strip_keeps_links_that_point_up_inside_what_it_keeps() {
    let root = tempfile::tempdir().unwrap();
    let tarball = |link: &str| {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        builder
            .append_data(
                &mut header,
                "node-v24/lib/node_modules/npm/bin/npm-cli.js",
                &b"npm"[..],
            )
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, "node-v24/bin/npm", link)
            .unwrap();
        let path = root.path().join("node.tar");
        std::fs::write(&path, builder.into_inner().unwrap()).unwrap();
        path
    };
    let node = root.path().join("node");
    let cancellation = CancellationToken::default();
    let archive = tarball("../lib/node_modules/npm/bin/npm-cli.js");
    files::unpack(&archive, &component(&node, ""), 1, false, &cancellation).unwrap();
    assert_eq!(std::fs::read(node.join("bin/npm")).unwrap(), b"npm");
    assert!(
        std::fs::symlink_metadata(node.join("bin/npm"))
            .unwrap()
            .is_symlink()
    );

    // Out of `node-v24`, which is all that is kept, and out of the archive.
    for link in ["../../other/npm-cli.js", "../../../npm-cli.js"] {
        let into = root.path().join("refused");
        let error = failure(files::unpack(
            &tarball(link),
            &component(&into, ""),
            1,
            false,
            &cancellation,
        ));
        assert!(error.contains("unsafe tar link target"), "{error}");
        assert!(!into.exists());
    }
}

fn path_entries(env: &BTreeMap<String, String>) -> Vec<PathBuf> {
    std::env::split_paths(&env["PATH"]).collect()
}

#[test]
fn a_types_path_entries_are_a_caller_overlay_under_the_managed_python() {
    let base = PathBuf::from(if cfg!(windows) { r"C:\i" } else { "/i" });
    let (node, node_bin) = (base.join("node"), base.join("node").join("bin"));
    let caller = base.join("caller");
    let python = base.join("python");
    let system = std::env::join_paths([base.join("system")]).unwrap();
    let environment = ProcessEnvironment {
        unset: Vec::new(),
        vars: BTreeMap::from([
            ("TOOL".to_string(), "type".to_string()),
            ("SHARED".to_string(), "type".to_string()),
        ]),
        path: vec![node.clone(), node_bin.clone()],
        launcher_code: None,
    };
    let configured = std::env::join_paths([caller.clone(), node.clone()]).unwrap();
    let instance = BTreeMap::from([
        ("SHARED".to_string(), "instance".to_string()),
        ("PATH".to_string(), configured.into_string().unwrap()),
    ]);
    let managed = |env| {
        crate::internal::hanafubuki_webui::runtime::merge_python_path_environment(
            env,
            &python,
            Some(system.as_os_str()),
        )
    };
    let env = environment
        .compose(&instance, Some(system.as_os_str()), managed)
        .unwrap();
    assert_eq!(env["TOOL"], "type");
    assert_eq!(env["SHARED"], "instance");
    // The managed Python first, then the type's entries, then the instance's
    // PATH, which replaces the process's.
    let mut expected =
        crate::internal::hanafubuki_webui::runtime::python_path_entries(&python).unwrap();
    expected.extend([node.clone(), node_bin.clone(), caller]);
    assert_eq!(path_entries(&env), expected);

    let env = environment
        .compose(&BTreeMap::new(), Some(system.as_os_str()), Ok)
        .unwrap();
    assert_eq!(path_entries(&env), [node, node_bin, base.join("system")]);
    // A type without entries leaves PATH to the process.
    let env = ProcessEnvironment::default()
        .compose(&BTreeMap::new(), Some(OsStr::new("ignored")), Ok)
        .unwrap();
    assert!(env.is_empty());
}

#[test]
fn a_host_python_program_imports_the_launcher_code_without_the_hotpatcher() {
    let launcher_code = PathBuf::from(if cfg!(windows) {
        r"C:\runtime\launcher-code"
    } else {
        "/runtime/launcher-code"
    });
    let mine = if cfg!(windows) { r"C:\mine" } else { "/mine" };
    let instance = BTreeMap::from([
        ("PYTHONPATH".to_string(), mine.to_string()),
        ("PYTHONUTF8".to_string(), "0".to_string()),
        (
            "SD_WEBUI_ALL_IN_ONE_HOTPATCHER_CONFIG".to_string(),
            "forged".to_string(),
        ),
    ]);
    let host_python = ProcessEnvironment {
        launcher_code: Some(launcher_code.clone()),
        ..ProcessEnvironment::default()
    };
    let env = host_python.compose(&instance, None, Ok).unwrap();
    let pythonpath: Vec<PathBuf> = std::env::split_paths(&env["PYTHONPATH"]).collect();
    assert_eq!(pythonpath.first(), Some(&launcher_code));
    assert_eq!(pythonpath.last(), Some(&PathBuf::from(mine)));
    for name in ["PYTHONUTF8", "PYTHONUNBUFFERED", "PYTHONNOUSERSITE"] {
        assert_eq!(env[name], "1", "{name}");
    }
    assert!(
        env.keys()
            .all(|name| !name.starts_with("SD_WEBUI_ALL_IN_ONE_HOTPATCHER_"))
    );
    // Any other program runs with the instance's variables as they are.
    assert_eq!(
        ProcessEnvironment::default()
            .compose(&instance, None, Ok)
            .unwrap(),
        instance
    );
}

fn staging_left(parent: &Path) -> Vec<String> {
    std::fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".unpack-"))
        .collect()
}

#[test]
fn a_replacing_unpack_swaps_the_whole_folder_in() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    std::fs::create_dir_all(node.join("lib/old-module")).unwrap();
    std::fs::write(node.join("lib/old-module/index.js"), b"old").unwrap();
    std::fs::write(node.join("VERSION"), b"23").unwrap();
    let archive = root.path().join("node-v24.zip");
    std::fs::write(
        &archive,
        zip_of(&[("node-v24/VERSION", b"24"), ("node-v24/bin/node", b"node")]),
    )
    .unwrap();

    files::unpack(
        &archive,
        &component(&node, ""),
        1,
        true,
        &CancellationToken::default(),
    )
    .unwrap();

    assert_eq!(std::fs::read(node.join("VERSION")).unwrap(), b"24");
    assert_eq!(std::fs::read(node.join("bin/node")).unwrap(), b"node");
    // Nothing of the old version is left, nor a staging folder beside it.
    assert!(!node.join("lib").exists());
    assert!(staging_left(root.path()).is_empty());
}

#[test]
fn a_failed_or_cancelled_replacing_unpack_leaves_the_old_folder() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    std::fs::create_dir_all(&node).unwrap();
    std::fs::write(node.join("VERSION"), b"23").unwrap();
    let broken = root.path().join("broken.zip");
    std::fs::write(&broken, b"not a zip archive").unwrap();

    failure(files::unpack(
        &broken,
        &component(&node, ""),
        0,
        true,
        &CancellationToken::default(),
    ));
    assert_eq!(std::fs::read(node.join("VERSION")).unwrap(), b"23");

    let good = root.path().join("good.zip");
    std::fs::write(&good, zip_of(&[("VERSION", b"24")])).unwrap();
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    let outcome = files::unpack(&good, &component(&node, ""), 0, true, &cancelled);
    assert!(
        matches!(outcome, Err(InstanceInstallStageError::Cancelled)),
        "{outcome:?}"
    );
    assert_eq!(std::fs::read(node.join("VERSION")).unwrap(), b"23");
    assert!(staging_left(root.path()).is_empty());
}

#[test]
fn a_replacing_unpack_creates_a_missing_folder() {
    let root = tempfile::tempdir().unwrap();
    let core = root.path().join("core");
    let archive = root.path().join("app.zip");
    std::fs::write(&archive, zip_of(&[("app.js", b"app")])).unwrap();

    files::unpack(
        &archive,
        &component(&core, "app"),
        0,
        true,
        &CancellationToken::default(),
    )
    .unwrap();

    assert_eq!(std::fs::read(core.join("app/app.js")).unwrap(), b"app");
}
