//! Declared candidates: the signatures registered types give the scanner.

use std::collections::BTreeMap;

use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use serde_json::json;

use super::*;
use crate::internal::hanafubuki_webui::discovery::{
    DeclaredDiscoveryCandidate, DiscoverySignature, RelativePath,
};

fn signature(
    type_id: &str,
    markers: &[&str],
    root: &str,
    components: &[(&str, &str)],
) -> DiscoverySignature {
    DiscoverySignature {
        type_id: InstanceTypeId::parse(type_id).unwrap(),
        markers: markers
            .iter()
            .map(|marker| RelativePath::parse(marker).unwrap())
            .collect(),
        root: RelativePath::parse(root).unwrap(),
        components: components
            .iter()
            .map(|(name, path)| (name.to_string(), RelativePath::parse(path).unwrap()))
            .collect(),
        values: json!({ "version": "latest" }).as_object().unwrap().clone(),
        external: Default::default(),
    }
}

fn dsh() -> DiscoverySignature {
    signature(
        "test.ext:dsh",
        &["node_modules/@deepseek-ai/dsh/package.json"],
        ".",
        &[("core", "."), ("node", "node")],
    )
}

fn declared(report: &WebUiDiscoveryReport) -> Vec<&DeclaredDiscoveryCandidate> {
    report
        .candidates
        .iter()
        .filter_map(|candidate| match candidate {
            WebUiDiscoveryCandidate::Declared(candidate) => Some(candidate),
            WebUiDiscoveryCandidate::Builtin(_) => None,
        })
        .collect()
}

#[test]
fn a_signature_reports_its_root_components_and_values_and_its_subtree_is_not_walked() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "dsh/node_modules/@deepseek-ai/dsh/package.json");
    directory(root, "dsh/node");
    // Below the match: neither a second match nor a built-in layout is seen.
    file(
        root,
        "dsh/node_modules/nested/node_modules/@deepseek-ai/dsh/package.json",
    );
    file(root, "dsh/node_modules/comfy/main.py");
    file(root, "dsh/node_modules/comfy/nodes.py");
    directory(root, "dsh/node_modules/comfy/comfy");
    file(root, "other/node_modules/@deepseek-ai/dsh/README.md");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[dsh()],
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(scan.candidates.len(), 1, "{:?}", scan.candidates);
    let candidate = declared(&scan)[0];
    assert_eq!(candidate.type_id.as_str(), "test.ext:dsh");
    assert_eq!(
        candidate.root.native(),
        resolved(&root.join("dsh")).native().to_path_buf()
    );
    let components: BTreeMap<&str, &Path> = candidate
        .component_paths
        .iter()
        .map(|(name, path)| (name.as_str(), path.native()))
        .collect();
    assert_eq!(
        components,
        BTreeMap::from([
            ("core", resolved(&root.join("dsh")).native()),
            ("node", resolved(&root.join("dsh/node")).native()),
        ])
    );
    assert_eq!(candidate.values["version"], "latest");
    assert_eq!(
        scan.candidates[0].type_id(),
        InstanceTypeId::parse("test.ext:dsh").unwrap()
    );
}

#[test]
fn a_built_in_layout_wins_and_no_signature_is_tried_inside_it() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    let llama = signature("test.ext:llama", &["main.py"], ".", &[]);
    file(root, "ComfyUI/main.py");
    file(root, "ComfyUI/nodes.py");
    directory(root, "ComfyUI/comfy");
    file(root, "ComfyUI/custom_nodes/runner/main.py");
    file(root, "llama/main.py");

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[llama],
        &CancellationToken::default(),
        |_| {},
    );
    assert_eq!(scan.candidates.len(), 2, "{:?}", scan.candidates);
    let WebUiDiscoveryCandidate::Builtin(comfy) = &scan.candidates[0] else {
        panic!("the built-in layout wins: {:?}", scan.candidates[0]);
    };
    assert_eq!(comfy.family, WebUiFamily::Comfyui);
    assert_eq!(
        declared(&scan)[0].root.native(),
        resolved(&root.join("llama")).native().to_path_buf()
    );
}

#[test]
fn every_marker_must_exist_and_the_first_matching_signature_wins() {
    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let root = temporary.path();
    file(root, "both/llama-server");
    file(root, "both/llama-cli");
    file(root, "one/llama-server");
    let both = signature("test.ext:both", &["llama-server", "llama-cli"], ".", &[]);
    let wrapped = signature("test.ext:wrapped", &["llama-server"], "bin", &[]);

    let scan = scan_webui_roots(
        &[resolved(root)],
        &[both, wrapped],
        &CancellationToken::default(),
        |_| {},
    );
    let found: Vec<(&str, PathBuf)> = declared(&scan)
        .into_iter()
        .map(|candidate| {
            (
                candidate.type_id.as_str(),
                candidate.root.native().to_path_buf(),
            )
        })
        .collect();
    assert_eq!(
        found,
        vec![
            (
                "test.ext:both",
                resolved(&root.join("both")).native().to_path_buf()
            ),
            (
                "test.ext:wrapped",
                resolved(&root.join("one")).native().join("bin")
            ),
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_marker_reached_through_a_link_is_not_there() {
    use std::os::unix::fs::symlink;

    let temporary = crate::internal::hanafubuki_webui::test_support::tempdir();
    let outside = directory(temporary.path(), "outside");
    file(&outside, "@deepseek-ai/dsh/package.json");
    let root = directory(temporary.path(), "root");
    directory(&root, "dsh/node_modules");
    symlink(
        outside.join("@deepseek-ai"),
        root.join("dsh/node_modules/@deepseek-ai"),
    )
    .unwrap();
    directory(&root, "linked");
    symlink(
        outside.join("@deepseek-ai/dsh"),
        root.join("linked/package"),
    )
    .unwrap();
    let marker_link = signature("test.ext:link", &["package/package.json"], ".", &[]);

    let scan = scan_webui_roots(
        &[resolved(&root)],
        &[dsh(), marker_link],
        &CancellationToken::default(),
        |_| {},
    );
    assert!(scan.candidates.is_empty(), "{:?}", scan.candidates);
}
