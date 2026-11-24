use super::*;
use serde_json::json;

#[test]
fn objects_merge_recursively() {
    let base = json!({"a": 1, "nested": {"x": 1, "y": 2}});
    let overlay = json!({"b": 2, "nested": {"y": 20, "z": 30}});
    let merged = merge_json_keep_null(base, overlay);
    assert_eq!(
        merged,
        json!({"a": 1, "b": 2, "nested": {"x": 1, "y": 20, "z": 30}})
    );
}

#[test]
fn arrays_are_replaced_wholesale() {
    let base = json!({"list": [1, 2, 3]});
    let overlay = json!({"list": [9]});
    assert_eq!(merge_json_keep_null(base, overlay), json!({"list": [9]}));
}

#[test]
fn scalar_overlay_replaces_base() {
    assert_eq!(merge_json_keep_null(json!(1), json!("two")), json!("two"));
}

#[test]
fn keep_null_replaces_key_with_null() {
    let base = json!({"customGithubMirror": "https://old", "other": 1});
    let overlay = json!({"customGithubMirror": null});
    assert_eq!(
        merge_json_keep_null(base, overlay),
        json!({"customGithubMirror": null, "other": 1})
    );
}

#[test]
fn remove_null_removes_key() {
    let base = json!({"keep": 1, "drop": 2});
    let overlay = json!({"drop": null});
    assert_eq!(merge_json_remove_null(base, overlay), json!({"keep": 1}));
}
