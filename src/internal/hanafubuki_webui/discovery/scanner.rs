use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, DirEntry, FileType};
use std::time::{Duration, Instant};

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::paths::{PathKey, ResolvedPath};

use super::signatures::DirectoryEntries;
use super::{
    BuiltinDiscoveryCandidate, DeclaredDiscoveryCandidate, DiscoverySignature,
    MAX_DISCOVERY_WARNINGS, WebUiDiscoveryCandidate, WebUiDiscoveryProgress, WebUiDiscoveryReport,
    WebUiDiscoveryWarning,
};
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

enum CandidateShape {
    Builtin {
        family: WebUiFamily,
        core_path: ResolvedPath,
        parent_path: Option<ResolvedPath>,
    },
    Declared(DeclaredDiscoveryCandidate),
}

enum PythonAssociation {
    Found(ResolvedPath),
    Missing,
    Ambiguous,
}

/// Walks `roots` for existing installations. A directory is first matched
/// against the built-in families' layouts, whose subtrees are still walked
/// (a portable Python lies below a core); then, outside every candidate
/// found so far, against `signatures` in order, the first that matches
/// making a declared candidate whose subtree is not walked.
pub fn scan_webui_roots(
    roots: &[ResolvedPath],
    signatures: &[DiscoverySignature],
    cancellation: &CancellationToken,
    mut report_progress: impl FnMut(WebUiDiscoveryProgress),
) -> WebUiDiscoveryReport {
    let roots = reduce_roots(roots);
    let mut report = WebUiDiscoveryReport::default();
    let mut stack = roots
        .iter()
        .rev()
        .cloned()
        .map(|path| {
            let filesystem = root_filesystem(&path);
            (path, true, filesystem)
        })
        .collect::<Vec<_>>();
    let mut candidates = BTreeMap::<PathKey, CandidateShape>::new();
    let mut python_runtime_directories = BTreeMap::<PathKey, ResolvedPath>::new();
    let mut last_progress = Instant::now();

    while let Some((directory, requested_root, root_filesystem)) = stack.pop() {
        if cancellation.is_cancelled() {
            report.cancelled = true;
            break;
        }
        report.directories += 1;
        let entries = match fs::read_dir(directory.system()) {
            Ok(entries) => entries,
            Err(error) => {
                push_scan_issue(&mut report, requested_root, &directory, error.to_string());
                continue;
            }
        };
        let mut files = HashSet::new();
        let mut directories = HashSet::new();
        let mut children = Vec::new();
        for entry in entries {
            if cancellation.is_cancelled() {
                report.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    push_warning(
                        &mut report,
                        directory.native_text().to_string(),
                        format!("read directory entry: {error}"),
                    );
                    continue;
                }
            };
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    push_warning(
                        &mut report,
                        entry.path().to_string_lossy().into_owned(),
                        format!("inspect directory entry: {error}"),
                    );
                    continue;
                }
            };
            match entry_is_link_like(&entry, &file_type) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    push_warning(
                        &mut report,
                        entry.path().to_string_lossy().into_owned(),
                        error,
                    );
                    continue;
                }
            }
            let name = entry.file_name();
            if file_type.is_dir() {
                if let Some(text) = name.to_str() {
                    directories.insert(text.to_string());
                }
                match ResolvedPath::resolve_child(&directory, &name) {
                    Ok(child) => {
                        match remains_on_root_filesystem(&child, root_filesystem) {
                            Ok(true) => {}
                            Ok(false) => continue,
                            Err(error) => {
                                push_warning(&mut report, child.native_text().to_string(), error);
                                continue;
                            }
                        }
                        children.push((child, false, root_filesystem));
                    }
                    Err(error) => push_warning(
                        &mut report,
                        entry.path().to_string_lossy().into_owned(),
                        error.to_string(),
                    ),
                }
            } else if file_type.is_file() {
                report.files += 1;
                if let Some(text) = name.to_str() {
                    files.insert(text.to_string());
                }
            }
            if last_progress.elapsed() >= Duration::from_millis(100) {
                emit_progress(&mut report_progress, &report, &directory, candidates.len());
                last_progress = Instant::now();
            }
        }
        if report.cancelled {
            break;
        }
        if let Some(python_dir) = python_runtime_root(&directory, &files) {
            python_runtime_directories
                .entry(python_dir.key().clone())
                .or_insert(python_dir);
        }
        if let Some(family) = match_family(&files, &directories) {
            let parent_path = directory
                .native()
                .parent()
                .and_then(|parent| ResolvedPath::resolve(parent).ok());
            candidates
                .entry(directory.key().clone())
                .or_insert_with(|| CandidateShape::Builtin {
                    family,
                    core_path: directory.clone(),
                    parent_path,
                });
            emit_progress(&mut report_progress, &report, &directory, candidates.len());
            last_progress = Instant::now();
        } else if !signatures.is_empty()
            && !candidates
                .keys()
                .any(|found| directory.key().starts_with(found))
        {
            let entries = DirectoryEntries {
                files: &files,
                directories: &directories,
                filesystem: root_filesystem,
            };
            let matched = signatures
                .iter()
                .find_map(|signature| signature.candidate(&directory, &entries));
            match matched {
                Some(Ok(candidate)) => {
                    candidates
                        .entry(candidate.root.key().clone())
                        .or_insert(CandidateShape::Declared(candidate));
                    emit_progress(&mut report_progress, &report, &directory, candidates.len());
                    last_progress = Instant::now();
                    continue;
                }
                Some(Err(error)) => {
                    push_warning(
                        &mut report,
                        directory.native_text().to_string(),
                        error.to_string(),
                    );
                }
                None => {}
            }
        }
        stack.extend(children);
    }

    for candidate in candidates.into_values() {
        let candidate = match candidate {
            CandidateShape::Builtin {
                family,
                core_path,
                parent_path,
            } => WebUiDiscoveryCandidate::Builtin(builtin_candidate(
                &mut report,
                family,
                core_path,
                parent_path,
                &python_runtime_directories,
            )),
            CandidateShape::Declared(candidate) => WebUiDiscoveryCandidate::Declared(candidate),
        };
        report.candidates.push(candidate);
    }
    report.candidates.sort_by(|left, right| {
        left.path()
            .key()
            .cmp(right.path().key())
            .then_with(|| left.path().native_text().cmp(right.path().native_text()))
    });
    if let Some(root) = roots.first() {
        emit_progress(&mut report_progress, &report, root, report.candidates.len());
    }
    report
}

/// A built-in layout with the portable Python it is associated with, when
/// there is exactly one, and the instance root that implies.
fn builtin_candidate(
    report: &mut WebUiDiscoveryReport,
    family: WebUiFamily,
    core_path: ResolvedPath,
    parent_path: Option<ResolvedPath>,
    python_runtime_directories: &BTreeMap<PathKey, ResolvedPath>,
) -> BuiltinDiscoveryCandidate {
    let python_dir =
        match associate_python(parent_path.as_ref(), &core_path, python_runtime_directories) {
            PythonAssociation::Found(path) => Some(path),
            PythonAssociation::Missing => None,
            PythonAssociation::Ambiguous => {
                push_warning(
                    report,
                    core_path.native_text().to_string(),
                    "multiple standalone Python runtimes were found directly below WebUI Core"
                        .to_string(),
                );
                None
            }
        };
    let instance_path = python_dir.as_ref().and_then(|python| {
        if python.key().starts_with(core_path.key()) {
            Some(core_path.clone())
        } else {
            parent_path.clone()
        }
    });
    BuiltinDiscoveryCandidate {
        family,
        instance_path,
        core_path,
        python_dir,
    }
}

fn reduce_roots(roots: &[ResolvedPath]) -> Vec<ResolvedPath> {
    let mut unique = BTreeMap::<PathKey, ResolvedPath>::new();
    for root in roots {
        unique
            .entry(root.key().clone())
            .or_insert_with(|| root.clone());
    }
    let mut ordered = unique.into_values().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.key()
            .len()
            .cmp(&right.key().len())
            .then_with(|| left.key().cmp(right.key()))
    });
    let mut reduced: Vec<ResolvedPath> = Vec::new();
    for root in ordered {
        if reduced
            .iter()
            .any(|base| root.key().starts_with(base.key()))
        {
            continue;
        }
        reduced.push(root);
    }
    reduced
}

fn associate_python(
    parent: Option<&ResolvedPath>,
    core: &ResolvedPath,
    python_runtime_directories: &BTreeMap<PathKey, ResolvedPath>,
) -> PythonAssociation {
    if let Ok(exact_internal) = ResolvedPath::resolve_child(core, OsStr::new("python")) {
        if let Some(path) = python_runtime_directories.get(exact_internal.key()) {
            return PythonAssociation::Found(path.clone());
        }
    }

    if let Some(parent) = parent {
        if let Ok(exact_sibling) = ResolvedPath::resolve_child(parent, OsStr::new("python")) {
            if let Some(path) = python_runtime_directories.get(exact_sibling.key()) {
                return PythonAssociation::Found(path.clone());
            }
        }
    }

    let mut direct_children = python_runtime_directories.values().filter(|path| {
        path.key().starts_with(core.key()) && path.key().len() == core.key().len() + 1
    });
    let Some(first) = direct_children.next() else {
        return PythonAssociation::Missing;
    };
    if direct_children.next().is_some() {
        PythonAssociation::Ambiguous
    } else {
        PythonAssociation::Found(first.clone())
    }
}

#[cfg(windows)]
fn python_runtime_root(directory: &ResolvedPath, files: &HashSet<String>) -> Option<ResolvedPath> {
    files
        .iter()
        .any(|name| name.eq_ignore_ascii_case("python.exe"))
        .then(|| directory.clone())
}

#[cfg(not(windows))]
fn python_runtime_root(directory: &ResolvedPath, files: &HashSet<String>) -> Option<ResolvedPath> {
    if directory.native().file_name() != Some(OsStr::new("bin"))
        || (!files.contains("python3") && !files.contains("python"))
    {
        return None;
    }
    directory
        .native()
        .parent()
        .and_then(|parent| ResolvedPath::resolve(parent).ok())
}

fn match_family(files: &HashSet<String>, directories: &HashSet<String>) -> Option<WebUiFamily> {
    let has_files = |names: &[&str]| names.iter().all(|name| files.contains(*name));
    let has_directories = |names: &[&str]| names.iter().all(|name| directories.contains(*name));
    if (has_directories(&["mikazuki"]) && has_files(&["gui.py"]))
        || (has_directories(&["kohya_gui"]) && has_files(&["kohya_gui.py"]))
    {
        Some(WebUiFamily::SdTrainer)
    } else if has_directories(&["qwen_tts_webui"]) && has_files(&["launch.py"]) {
        Some(WebUiFamily::QwenTts)
    } else if has_directories(&["rvc_next_webui"]) && has_files(&["launch.py"]) {
        Some(WebUiFamily::RvcNext)
    } else if has_directories(&["mizuoto_tts_webui"]) && has_files(&["launch.py"]) {
        Some(WebUiFamily::MizuotoTts)
    } else if has_directories(&["ldm_patched", "presets", "wildcards"]) && has_files(&["launch.py"])
    {
        Some(WebUiFamily::Fooocus)
    } else if has_directories(&["modules"]) && has_files(&["launch.py", "webui.py"]) {
        Some(WebUiFamily::StableDiffusionWebui)
    } else if has_directories(&["comfy"]) && has_files(&["main.py", "nodes.py"]) {
        Some(WebUiFamily::Comfyui)
    } else if has_files(&["invokeai.yaml"]) {
        Some(WebUiFamily::Invokeai)
    } else {
        None
    }
}

fn emit_progress(
    sink: &mut impl FnMut(WebUiDiscoveryProgress),
    report: &WebUiDiscoveryReport,
    current: &ResolvedPath,
    webuis: usize,
) {
    sink(WebUiDiscoveryProgress {
        files: report.files,
        directories: report.directories,
        webuis: webuis as u64,
        current_path: current.native_text().to_string(),
    });
}

fn push_scan_issue(
    report: &mut WebUiDiscoveryReport,
    requested_root: bool,
    path: &ResolvedPath,
    message: String,
) {
    let issue = WebUiDiscoveryWarning {
        path: path.native_text().to_string(),
        message,
    };
    if requested_root {
        report.root_failures.push(issue);
    } else {
        push_warning(report, issue.path, issue.message);
    }
}

fn push_warning(report: &mut WebUiDiscoveryReport, path: String, message: String) {
    if report.warnings.len() < MAX_DISCOVERY_WARNINGS {
        report
            .warnings
            .push(WebUiDiscoveryWarning { path, message });
    } else {
        report.warnings_truncated = true;
    }
}

#[cfg(unix)]
fn root_filesystem(path: &ResolvedPath) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path.system())
        .ok()
        .map(|metadata| metadata.dev())
}

#[cfg(not(unix))]
fn root_filesystem(_path: &ResolvedPath) -> Option<u64> {
    None
}

#[cfg(unix)]
fn remains_on_root_filesystem(
    path: &ResolvedPath,
    root_filesystem: Option<u64>,
) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    let Some(root_filesystem) = root_filesystem else {
        return Ok(true);
    };
    fs::metadata(path.system())
        .map(|metadata| metadata.dev() == root_filesystem)
        .map_err(|error| format!("inspect directory filesystem: {error}"))
}

#[cfg(not(unix))]
fn remains_on_root_filesystem(
    _path: &ResolvedPath,
    _root_filesystem: Option<u64>,
) -> Result<bool, String> {
    Ok(true)
}

#[cfg(not(windows))]
fn entry_is_link_like(_entry: &DirEntry, file_type: &FileType) -> Result<bool, String> {
    Ok(file_type.is_symlink())
}

#[cfg(windows)]
fn entry_is_link_like(entry: &DirEntry, file_type: &FileType) -> Result<bool, String> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    if file_type.is_symlink() {
        return Ok(true);
    }
    entry
        .metadata()
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .map_err(|error| format!("inspect directory reparse attributes: {error}"))
}
