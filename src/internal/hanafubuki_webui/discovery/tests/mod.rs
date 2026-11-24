use std::fs;
use std::path::{Path, PathBuf};

use crate::internal::hanafubuki_foundation::cancellation::CancellationToken;
use crate::internal::hanafubuki_foundation::paths::ResolvedPath;

use super::{
    BuiltinDiscoveryCandidate, WebUiDiscoveryCandidate, WebUiDiscoveryReport, scan_webui_roots,
};
use crate::internal::hanafubuki_instance_contracts::WebUiFamily;

fn directory(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(&path).unwrap();
    path
}

fn file(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, []).unwrap();
}

fn python_runtime(root: &Path, relative: &str) -> PathBuf {
    let path = directory(root, relative);
    #[cfg(windows)]
    file(&path, "python.exe");
    #[cfg(not(windows))]
    file(&path, "bin/python");
    path
}

fn builtins(report: &WebUiDiscoveryReport) -> Vec<&BuiltinDiscoveryCandidate> {
    report
        .candidates
        .iter()
        .map(|candidate| match candidate {
            WebUiDiscoveryCandidate::Builtin(candidate) => candidate,
            WebUiDiscoveryCandidate::Declared(candidate) => {
                panic!("no signature was given: {candidate:?}")
            }
        })
        .collect()
}

fn resolved(path: &Path) -> ResolvedPath {
    ResolvedPath::resolve(path).unwrap()
}

#[test]
fn recognizes_all_families_and_associates_portable_python() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "sd/core/launch.py");
    file(root, "sd/core/webui.py");
    directory(root, "sd/core/modules");
    python_runtime(root, "sd/python");
    file(root, "comfy/core/main.py");
    file(root, "comfy/core/nodes.py");
    directory(root, "comfy/core/comfy");
    python_runtime(root, "comfy/python");
    file(root, "invoke/core/invokeai.yaml");
    file(root, "fooocus/core/launch.py");
    directory(root, "fooocus/core/ldm_patched");
    directory(root, "fooocus/core/presets");
    directory(root, "fooocus/core/wildcards");
    file(root, "trainer-one/core/gui.py");
    directory(root, "trainer-one/core/mikazuki");
    file(root, "trainer-two/core/kohya_gui.py");
    directory(root, "trainer-two/core/kohya_gui");
    file(root, "qwen/core/launch.py");
    directory(root, "qwen/core/qwen_tts_webui");
    file(root, "rvc/core/launch.py");
    directory(root, "rvc/core/rvc_next_webui");
    directory(root, "rvc/core/data");
    file(root, "mizuoto/core/launch.py");
    directory(root, "mizuoto/core/mizuoto_tts_webui");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 9);
    let families = candidates
        .iter()
        .map(|candidate| candidate.family)
        .collect::<Vec<_>>();
    assert!(families.contains(&WebUiFamily::StableDiffusionWebui));
    assert!(families.contains(&WebUiFamily::Comfyui));
    assert!(families.contains(&WebUiFamily::Invokeai));
    assert!(families.contains(&WebUiFamily::Fooocus));
    assert_eq!(
        families
            .iter()
            .filter(|family| **family == WebUiFamily::SdTrainer)
            .count(),
        2
    );
    assert!(families.contains(&WebUiFamily::QwenTts));
    assert!(families.contains(&WebUiFamily::RvcNext));
    assert!(families.contains(&WebUiFamily::MizuotoTts));
    let sd = candidates
        .iter()
        .find(|candidate| candidate.family == WebUiFamily::StableDiffusionWebui)
        .unwrap();
    assert_eq!(
        sd.python_dir.as_ref().unwrap().native(),
        resolved(&root.join("sd/python")).native().to_path_buf()
    );
    assert_eq!(
        sd.instance_path.as_ref().unwrap().native(),
        resolved(&root.join("sd")).native().to_path_buf()
    );
    let comfy = candidates
        .iter()
        .find(|candidate| candidate.family == WebUiFamily::Comfyui)
        .unwrap();
    assert_eq!(
        comfy.python_dir.as_ref().unwrap().native(),
        resolved(&root.join("comfy/python")).native().to_path_buf()
    );
    assert_eq!(
        comfy.instance_path.as_ref().unwrap().native(),
        resolved(&root.join("comfy")).native().to_path_buf()
    );
}

#[test]
fn rejects_partial_shapes_and_accepts_direct_python_inside_core() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "partial/core/launch.py");
    directory(root, "partial/core/modules");
    file(root, "qwen/core/launch.py");
    directory(root, "qwen/core/qwen_tts_webui");
    python_runtime(root, "qwen/core/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(candidates[0].family, WebUiFamily::QwenTts);
    assert_eq!(
        candidates[0].python_dir.as_ref().unwrap().native(),
        resolved(&root.join("qwen/core/python"))
            .native()
            .to_path_buf()
    );
    assert_eq!(
        candidates[0].instance_path.as_ref().unwrap().native(),
        resolved(&root.join("qwen/core")).native().to_path_buf()
    );
}

#[test]
fn python_inside_core_wins_over_a_sibling_python() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "comfy/core/main.py");
    file(root, "comfy/core/nodes.py");
    directory(root, "comfy/core/comfy");
    python_runtime(root, "comfy/core/python");
    python_runtime(root, "comfy/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        candidates[0].python_dir.as_ref().unwrap().native(),
        resolved(&root.join("comfy/core/python"))
            .native()
            .to_path_buf()
    );
    assert_eq!(
        candidates[0].instance_path.as_ref().unwrap().native(),
        resolved(&root.join("comfy/core")).native().to_path_buf()
    );
}

#[test]
fn a_webui_directory_not_named_core_can_own_its_nested_python() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "ComfyUI/main.py");
    file(root, "ComfyUI/nodes.py");
    directory(root, "ComfyUI/comfy");
    python_runtime(root, "ComfyUI/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        candidates[0].core_path.native(),
        resolved(&root.join("ComfyUI")).native().to_path_buf()
    );
    assert_eq!(
        candidates[0].python_dir.as_ref().unwrap().native(),
        resolved(&root.join("ComfyUI/python"))
            .native()
            .to_path_buf()
    );
    assert_eq!(
        candidates[0].instance_path.as_ref().unwrap().native(),
        resolved(&root.join("ComfyUI")).native().to_path_buf()
    );
}

#[test]
fn associates_a_unique_nonstandard_python_runtime_directly_below_core() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "ComfyUI/main.py");
    file(root, "ComfyUI/nodes.py");
    directory(root, "ComfyUI/comfy");
    directory(root, "ComfyUI/python");
    python_runtime(root, "ComfyUI/py311");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        candidates[0].python_dir.as_ref().unwrap().native(),
        resolved(&root.join("ComfyUI/py311")).native().to_path_buf()
    );
    assert_eq!(
        candidates[0].instance_path.as_ref().unwrap().native(),
        resolved(&root.join("ComfyUI")).native().to_path_buf()
    );
}

#[test]
fn standard_sibling_python_wins_over_a_nonstandard_core_runtime() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "comfy/core/main.py");
    file(root, "comfy/core/nodes.py");
    directory(root, "comfy/core/comfy");
    python_runtime(root, "comfy/core/py3");
    python_runtime(root, "comfy/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(
        candidates[0].python_dir.as_ref().unwrap().native(),
        resolved(&root.join("comfy/python")).native().to_path_buf()
    );
    assert_eq!(
        candidates[0].instance_path.as_ref().unwrap().native(),
        resolved(&root.join("comfy")).native().to_path_buf()
    );
}

#[test]
fn ambiguous_nonstandard_core_runtimes_leave_the_candidate_incomplete() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "ComfyUI/main.py");
    file(root, "ComfyUI/nodes.py");
    directory(root, "ComfyUI/comfy");
    python_runtime(root, "ComfyUI/py3");
    python_runtime(root, "ComfyUI/py311");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert!(candidates[0].python_dir.is_none());
    assert!(candidates[0].instance_path.is_none());
    assert!(scan.warnings.iter().any(|warning| {
        warning
            .message
            .contains("multiple standalone Python runtimes")
    }));
}

#[test]
fn ignores_python_nested_below_core() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "ComfyUI/main.py");
    file(root, "ComfyUI/nodes.py");
    directory(root, "ComfyUI/comfy");
    python_runtime(root, "ComfyUI/custom_nodes/plugin/codes/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(candidates[0].family, WebUiFamily::Comfyui);
    assert!(candidates[0].python_dir.is_none());
    assert!(candidates[0].instance_path.is_none());
}

#[test]
fn ignores_python_nested_elsewhere_below_core_parent() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "InvokeAI/invokeai.yaml");
    python_runtime(root, "Installer/comfyui/python");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    let candidates = builtins(&scan);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(candidates[0].family, WebUiFamily::Invokeai);
    assert!(candidates[0].python_dir.is_none());
    assert!(candidates[0].instance_path.is_none());
}

#[test]
fn cancellation_returns_a_cancelled_report() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    directory(temporary.path(), "one/two/three");
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let scan = scan_webui_roots(&[resolved(temporary.path())], &[], &cancellation, |_| {});
    assert!(scan.cancelled);
    assert!(scan.candidates.is_empty());
}

#[cfg(unix)]
#[test]
fn symbolic_link_trees_are_not_followed() {
    use std::os::unix::fs::symlink;

    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let outside = directory(temporary.path(), "outside");
    file(&outside, "main.py");
    file(&outside, "nodes.py");
    directory(&outside, "comfy");
    let root = directory(temporary.path(), "root");
    symlink(&outside, root.join("linked")).unwrap();
    let scan = scan_webui_roots(
        &[resolved(&root)],
        &[],
        &CancellationToken::default(),
        |_| {},
    );
    assert!(scan.candidates.is_empty());
}

mod signatures;
