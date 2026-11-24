use std::sync::Arc;

use crate::internal::hanafubuki_foundation::form_schema::SchemaText;
use crate::internal::hanafubuki_foundation::module_faults::ContributionOwner;
use crate::internal::hanafubuki_instance_contracts::InstanceTypeId;
use serde_json::{Value, json};

use super::{
    ContributedPresets, InstancePreset, LaunchArgument, LaunchValue, PresetCatalog, PresetId,
    PresetOwner, SaveInstancePresetRequest, merge_launch_arguments,
};
use crate::internal::hanafubuki_webui::instance_types::declarative_fixtures::TWO_SERVERS;
use crate::internal::hanafubuki_webui::instance_types::{
    DeclarativeProvider, InstanceTypeRegistry,
};

const EXTENSION: &str = "com.example.presets";

const LOW_VRAM: &str = r#"
id = "low-vram"
base = "builtin:comfyui"
label = { en = "ComfyUI (low VRAM)", zh = "ComfyUI（低显存）" }
description = { en = "For small GPUs." }
futureField = "ignored by this build"

[install]
pythonVersion = "3.11"

[launch]
arguments = [{ name = "--lowvram", value = true }, { name = "--preview-method", value = "taesd" }]
"#;

/// A type whose form has a secret beside a plain field.
const KEYED: &str = r#"
name = "keyed"
displayName = "Keyed"
components = ["core"]

[[install.parameters]]
name = "endpoint"
kind = "text"

[[install.parameters]]
name = "token"
kind = "secret"

[[install.steps]]
kind = "write-file"
path = "{component.core}/endpoint.txt"
content = "{param.endpoint}"
"#;

fn keyed() -> InstanceTypeId {
    InstanceTypeId::parse("com.example:keyed").unwrap()
}

fn comfyui() -> InstanceTypeId {
    InstanceTypeId::parse("builtin:comfyui").unwrap()
}

fn two_servers() -> InstanceTypeId {
    InstanceTypeId::parse("com.example:two-servers").unwrap()
}

struct Fixture {
    _root: tempfile::TempDir,
    file: std::path::PathBuf,
    contributed: Arc<ContributedPresets>,
    catalog: PresetCatalog,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let types = Arc::new(InstanceTypeRegistry::with_builtin_types());
    let declarative = DeclarativeProvider::from_toml("com.example", TWO_SERVERS).unwrap();
    types
        .register(
            ContributionOwner::Extension("com.example".to_string()),
            Arc::new(declarative),
        )
        .unwrap();
    let keyed = DeclarativeProvider::from_toml("com.example", KEYED).unwrap();
    types
        .register(
            ContributionOwner::Extension("com.example".to_string()),
            Arc::new(keyed),
        )
        .unwrap();
    let contributed = Arc::new(ContributedPresets::default());
    let file = root.path().join("config").join("instance-presets.json");
    let catalog = PresetCatalog::new(types, contributed.clone(), file.clone());
    Fixture {
        _root: root,
        file,
        contributed,
        catalog,
    }
}

fn extension_preset(text: &str) -> InstancePreset {
    InstancePreset::from_toml(EXTENSION, text).unwrap()
}

fn owner() -> ContributionOwner {
    ContributionOwner::Extension(EXTENSION.to_string())
}

fn flag(name: &str, value: bool) -> LaunchArgument {
    LaunchArgument {
        name: name.to_string(),
        value: LaunchValue::Flag(value),
    }
}

fn text(name: &str, value: &str) -> LaunchArgument {
    LaunchArgument {
        name: name.to_string(),
        value: LaunchValue::Text(value.to_string()),
    }
}

fn owned(tokens: &[&str]) -> Vec<String> {
    tokens.iter().map(|token| token.to_string()).collect()
}

#[test]
fn an_extension_preset_fills_its_base_types_form() {
    let fixture = fixture();
    let preset = extension_preset(LOW_VRAM);
    assert_eq!(
        preset.label,
        SchemaText::localized(
            [("en", "ComfyUI (low VRAM)"), ("zh", "ComfyUI（低显存）")]
                .map(|(locale, text)| (locale.to_string(), text.to_string()))
                .into()
        )
    );
    fixture.contributed.register(owner(), vec![preset]).unwrap();

    let listing = fixture.catalog.list(Some(&comfyui())).unwrap();
    let [entry] = listing.presets.as_slice() else {
        panic!("{listing:?}");
    };
    assert_eq!(entry.id.to_string(), "com.example.presets:low-vram");
    assert_eq!(entry.owner, PresetOwner::Extension(EXTENSION.to_string()));
    assert_eq!(entry.unavailable, None);
    // Every field of the form: the preset's value over the field's default.
    assert_eq!(
        Value::Object(entry.values.clone()),
        json!({
            "pythonVersion": "3.11",
            "pytorch": null,
            "predownloadModels": null,
            "predownloadExtensions": null,
        })
    );
    assert_eq!(
        entry.launch_arguments,
        [flag("--lowvram", true), text("--preview-method", "taesd")]
    );
    assert!(
        fixture
            .catalog
            .list(Some(&two_servers()))
            .unwrap()
            .presets
            .is_empty()
    );
    assert_eq!(
        fixture.contributed.owned_by(&owner()),
        ["com.example.presets:low-vram"]
    );

    fixture.contributed.unregister(&owner());
    assert!(fixture.catalog.list(None).unwrap().presets.is_empty());
}

#[test]
fn a_preset_that_does_not_read_is_an_issue_at_its_place() {
    let issues = InstancePreset::from_toml(EXTENSION, "id = \"low-vram\"\nbase = 3\n").unwrap_err();
    assert_eq!(issues[0].location, "line 2, column 8");

    let issues = InstancePreset::from_toml(
        EXTENSION,
        r#"
id = "Low VRAM"
base = "comfyui"
label = { key = "app.other" }
[launch]
arguments = [{ name = "lowvram", value = true }]
"#,
    )
    .unwrap_err();
    let located: Vec<&str> = issues.iter().map(|issue| issue.location.as_str()).collect();
    assert_eq!(located, ["id"]);

    let issues = InstancePreset::from_toml(
        EXTENSION,
        r#"
id = "low-vram"
base = "comfyui"
label = { key = "app.other" }
[launch]
arguments = [{ name = "lowvram", value = true }]
"#,
    )
    .unwrap_err();
    let located: Vec<&str> = issues.iter().map(|issue| issue.location.as_str()).collect();
    assert_eq!(located, ["base", "label", "launch.arguments[0]"]);

    let issues =
        InstancePreset::from_json(EXTENSION, "{ \"base\": \"builtin:comfyui\" }").unwrap_err();
    assert!(issues[0].location.starts_with("line 1"), "{issues:?}");
}

#[test]
fn a_preset_over_an_unregistered_type_is_listed_unavailable() {
    let fixture = fixture();
    let gone = LOW_VRAM.replace("builtin:comfyui", "com.example.gone:type");
    fixture
        .contributed
        .register(owner(), vec![extension_preset(&gone)])
        .unwrap();
    let listing = fixture.catalog.list(None).unwrap();
    assert_eq!(listing.presets.len(), 1);
    assert_eq!(
        listing.presets[0].unavailable.as_deref(),
        Some("type `com.example.gone:type` is not available")
    );
    assert!(listing.presets[0].values.is_empty());
    let id = PresetId::parse("com.example.presets:low-vram").unwrap();
    let base = InstanceTypeId::parse("com.example.gone:type").unwrap();
    assert!(fixture.catalog.resolve(&id, &base).is_err());
}

#[test]
fn a_preset_setting_what_its_form_does_not_take_is_unusable() {
    let fixture = fixture();
    let undeclared = r#"
id = "odd"
base = "com.example:two-servers"
label = "Odd"
[install]
flavour = "spicy"
colour = "red"
[launch]
arguments = [{ name = "--fast", value = true }]
"#;
    fixture
        .contributed
        .register(owner(), vec![extension_preset(undeclared)])
        .unwrap();
    let listing = fixture.catalog.list(Some(&two_servers())).unwrap();
    assert_eq!(
        listing.presets[0].unavailable.as_deref(),
        Some(
            "`colour` is not a field of `com.example:two-servers`'s install form; \
             `flavour` takes one of plain, fancy; \
             type `com.example:two-servers` takes no launch arguments"
        )
    );
    let id = PresetId::parse("com.example.presets:odd").unwrap();
    let error = fixture.catalog.resolve(&id, &two_servers()).unwrap_err();
    assert!(
        error.starts_with("preset `com.example.presets:odd` cannot be used"),
        "{error}"
    );
}

#[test]
fn a_declarative_preset_leaves_required_fields_to_the_user() {
    let fixture = fixture();
    let fancy = r#"
id = "fancy"
base = "com.example:two-servers"
label = "Fancy"
[install]
flavour = "fancy"
"#;
    fixture
        .contributed
        .register(owner(), vec![extension_preset(fancy)])
        .unwrap();
    let entry = &fixture.catalog.list(Some(&two_servers())).unwrap().presets[0];
    assert_eq!(entry.unavailable, None);
    assert_eq!(
        Value::Object(entry.values.clone()),
        json!({ "archiveUrl": null, "sha256": null, "uiPort": 18080, "flavour": "fancy" })
    );
    let resolved = fixture
        .catalog
        .resolve(
            &PresetId::parse("com.example.presets:fancy").unwrap(),
            &two_servers(),
        )
        .unwrap();
    assert_eq!(
        Value::Object(resolved.install),
        json!({ "flavour": "fancy" })
    );
    assert!(
        fixture
            .catalog
            .resolve(
                &PresetId::parse("com.example.presets:fancy").unwrap(),
                &comfyui()
            )
            .unwrap_err()
            .contains("is over `com.example:two-servers`")
    );
}

#[test]
fn launch_arguments_merge_by_name() {
    let defaults = owned(&[
        "--auto-launch",
        "--preview-method",
        "auto",
        "--threshold",
        "-1",
        "--disable-cuda-malloc",
    ]);
    let merged = merge_launch_arguments(
        &defaults,
        &[
            text("--preview-method", "taesd"),
            flag("--disable-cuda-malloc", false),
            flag("--lowvram", true),
            LaunchArgument {
                name: "--port".to_string(),
                value: LaunchValue::Number(8188.into()),
            },
        ],
    );
    assert_eq!(
        merged,
        owned(&[
            "--auto-launch",
            "--preview-method",
            "taesd",
            "--threshold",
            "-1",
            "--lowvram",
            "--port",
            "8188",
        ])
    );
    assert_eq!(merge_launch_arguments(&defaults, &[]), defaults);
}

fn save(fixture: &Fixture, label: &str, values: Value) -> Result<super::PresetEntry, String> {
    fixture.catalog.save(SaveInstancePresetRequest {
        base: comfyui(),
        label: label.to_string(),
        description: None,
        values: values.as_object().unwrap().clone(),
        launch_arguments: vec![flag("--lowvram", true)],
    })
}

#[test]
fn the_users_presets_save_list_and_delete() {
    let fixture = fixture();
    let saved = save(&fixture, "My ComfyUI", json!({ "pythonVersion": "3.12" })).unwrap();
    assert_eq!(saved.id.to_string(), "user:my-comfyui");
    assert_eq!(saved.owner, PresetOwner::User);
    assert_eq!(saved.values["pythonVersion"], "3.12");
    assert_eq!(saved.launch_arguments, [flag("--lowvram", true)]);
    // A name with nothing ASCII is a name all the same; a repeat gets a suffix.
    assert_eq!(
        save(&fixture, "低显存", json!({})).unwrap().id.to_string(),
        "user:preset"
    );
    assert_eq!(
        save(&fixture, "My ComfyUI", json!({}))
            .unwrap()
            .id
            .to_string(),
        "user:my-comfyui-2"
    );
    assert_eq!(
        save(&fixture, "Bad", json!({ "colour": "red" })).unwrap_err(),
        "`colour` is not a field of `builtin:comfyui`'s install form"
    );
    assert_eq!(
        save(&fixture, "  ", json!({})).unwrap_err(),
        "a preset needs a name"
    );

    let listing = fixture.catalog.list(Some(&comfyui())).unwrap();
    let ids: Vec<String> = listing
        .presets
        .iter()
        .map(|entry| entry.id.to_string())
        .collect();
    assert_eq!(ids, ["user:my-comfyui", "user:my-comfyui-2", "user:preset"]);
    let resolved = fixture
        .catalog
        .resolve(&PresetId::parse("user:my-comfyui").unwrap(), &comfyui())
        .unwrap();
    assert_eq!(resolved.launch, [flag("--lowvram", true)]);

    fixture.catalog.delete("user:my-comfyui").unwrap();
    assert_eq!(
        fixture.catalog.delete("user:my-comfyui").unwrap_err(),
        "you have no preset `my-comfyui`"
    );
    assert!(
        fixture
            .catalog
            .delete("com.example.presets:low-vram")
            .unwrap_err()
            .contains("an extension's leaves with it")
    );
    assert_eq!(fixture.catalog.list(None).unwrap().presets.len(), 2);
}

#[test]
fn a_users_entry_that_does_not_read_is_kept_aside_and_the_file_quarantined() {
    let fixture = fixture();
    std::fs::create_dir_all(fixture.file.parent().unwrap()).unwrap();
    let written = json!({
        "presets": {
            "good": { "base": "builtin:comfyui", "label": "Good", "install": { "pythonVersion": "3.12" } },
            "no-base": { "label": "No base" },
            "Bad Key": { "base": "builtin:comfyui", "label": "Bad key" },
        },
        "futureSection": true,
    });
    std::fs::write(&fixture.file, written.to_string()).unwrap();

    let listing = fixture.catalog.list(None).unwrap();
    let ids: Vec<String> = listing
        .presets
        .iter()
        .map(|entry| entry.id.to_string())
        .collect();
    assert_eq!(ids, ["user:good"]);
    assert_eq!(listing.presets[0].unavailable, None);
    let broken: Vec<&str> = listing
        .broken
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(broken, ["user:Bad Key", "user:no-base"]);
    assert!(listing.broken[1].reason.contains("base"), "{listing:?}");

    // The original is kept in `.recovery`, and what each entry held stays in
    // the rewritten file.
    let recovery = fixture.file.parent().unwrap().join(".recovery");
    let kept: Vec<_> = std::fs::read_dir(&recovery).unwrap().collect();
    assert_eq!(kept.len(), 1);
    let rewritten: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.file).unwrap()).unwrap();
    assert_eq!(
        rewritten["broken"]["no-base"]["entry"],
        json!({ "label": "No base" })
    );
    assert_eq!(
        rewritten["presets"]["good"]["install"]["pythonVersion"],
        "3.12"
    );

    // Read again, nothing more is quarantined; a broken entry is deleted by its key.
    fixture.catalog.list(None).unwrap();
    assert_eq!(std::fs::read_dir(&recovery).unwrap().count(), 1);
    fixture.catalog.delete("user:no-base").unwrap();
    assert_eq!(fixture.catalog.list(None).unwrap().broken.len(), 1);

    // A document that is not one is quarantined whole.
    std::fs::write(&fixture.file, "[1, 2]").unwrap();
    let listing = fixture.catalog.list(None).unwrap();
    assert!(listing.presets.is_empty() && listing.broken.is_empty());
    assert_eq!(std::fs::read_dir(&recovery).unwrap().count(), 2);
}

#[test]
fn preset_ids_name_their_owner() {
    let user = PresetId::parse("user:low-vram").unwrap();
    assert_eq!(user.owner, PresetOwner::User);
    let extension = PresetId::parse("com.example.presets:low-vram").unwrap();
    assert_eq!(extension.to_string(), "com.example.presets:low-vram");
    assert!(PresetId::parse("low-vram").is_err());
    assert!(PresetId::parse("user:Low").is_err());
    assert!(PresetId::parse(":low").is_err());
}

#[test]
fn two_presets_of_one_name_are_refused() {
    let fixture = fixture();
    let error = fixture
        .contributed
        .register(
            owner(),
            vec![extension_preset(LOW_VRAM), extension_preset(LOW_VRAM)],
        )
        .unwrap_err();
    assert_eq!(error, "preset `low-vram` is contributed twice");
}

#[test]
fn save_as_preset_leaves_a_secret_out() {
    let fixture = fixture();
    let saved = fixture
        .catalog
        .save(SaveInstancePresetRequest {
            base: keyed(),
            label: "Staging".to_string(),
            description: None,
            values: json!({ "endpoint": "https://staging.test", "token": "s3cret" })
                .as_object()
                .unwrap()
                .clone(),
            launch_arguments: Vec::new(),
        })
        .unwrap();
    assert_eq!(saved.unavailable, None);
    assert_eq!(
        Value::Object(saved.values),
        json!({ "endpoint": "https://staging.test", "token": null })
    );
    let written = std::fs::read_to_string(&fixture.file).unwrap();
    assert!(!written.contains("s3cret"), "{written}");
    let listed = fixture.catalog.list(Some(&keyed())).unwrap();
    assert!(!serde_json::to_string(&listed).unwrap().contains("s3cret"));
}

#[test]
fn an_extension_preset_naming_a_secret_is_unusable() {
    let fixture = fixture();
    let preset = r#"
id = "with-token"
base = "com.example:keyed"
label = "With a token"
[install]
endpoint = "https://example.test"
token = "s3cret"
"#;
    fixture
        .contributed
        .register(owner(), vec![extension_preset(preset)])
        .unwrap();
    let listing = fixture.catalog.list(Some(&keyed())).unwrap();
    assert_eq!(
        listing.presets[0].unavailable.as_deref(),
        Some("`token` is a secret, which a preset never holds")
    );
    assert!(!serde_json::to_string(&listing).unwrap().contains("s3cret"));
    let id = PresetId::parse("com.example.presets:with-token").unwrap();
    assert!(fixture.catalog.resolve(&id, &keyed()).is_err());
}

#[test]
fn a_users_entry_naming_a_secret_is_unusable_and_kept() {
    let fixture = fixture();
    std::fs::create_dir_all(fixture.file.parent().unwrap()).unwrap();
    let written = json!({
        "presets": {
            "by-hand": { "base": "com.example:keyed", "label": "By hand", "install": { "token": "s3cret" } },
        },
    });
    std::fs::write(&fixture.file, written.to_string()).unwrap();
    let listing = fixture.catalog.list(None).unwrap();
    assert_eq!(listing.presets[0].id.to_string(), "user:by-hand");
    assert_eq!(
        listing.presets[0].unavailable.as_deref(),
        Some("`token` is a secret, which a preset never holds")
    );
    assert!(!serde_json::to_string(&listing).unwrap().contains("s3cret"));
    // The user's data stays as it was written.
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.file).unwrap()).unwrap();
    assert_eq!(kept, written);
}
