use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::{
    CommitMode, FieldKind, PathKind, SchemaCondition, SchemaField, SchemaOption, SchemaText,
    SchemaValue, check_fields, patched_values, read_values, stored_values,
};

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap()
}

fn option(value: &str) -> SchemaOption {
    SchemaOption {
        value: value.to_string(),
        label: SchemaText::literal(value),
    }
}

fn field(key: &str, kind: FieldKind) -> SchemaField {
    SchemaField::new(key, kind, SchemaText::literal(key))
}

#[test]
fn a_label_is_a_message_key_or_text_by_locale() {
    let keyed = SchemaField::new(
        "proxyMode",
        FieldKind::Select,
        SchemaText::key("settings.fields.proxyMode.label"),
    )
    .commit(CommitMode::Change)
    .patch_group("proxy")
    .options(vec![SchemaOption {
        value: "manual".to_string(),
        label: SchemaText::key_with(
            "settings.options.pythonVersion",
            BTreeMap::from([("version".to_string(), "3.11".to_string())]),
        ),
    }]);
    assert_eq!(
        serde_json::to_value(&keyed).unwrap(),
        json!({
            "key": "proxyMode",
            "kind": "select",
            "label": { "kind": "key", "key": "settings.fields.proxyMode.label" },
            "options": [{
                "value": "manual",
                "label": {
                    "kind": "key",
                    "key": "settings.options.pythonVersion",
                    "params": { "version": "3.11" }
                }
            }],
            "commit": "change",
            "patchGroup": "proxy"
        })
    );

    let localized = SchemaField::new(
        "model",
        FieldKind::Path,
        SchemaText::localized(BTreeMap::from([
            ("en".to_string(), "Model".to_string()),
            ("zh".to_string(), "模型".to_string()),
        ])),
    );
    let mut localized = localized;
    localized.path_kind = Some(PathKind::File);
    assert_eq!(
        serde_json::to_value(&localized).unwrap(),
        json!({
            "key": "model",
            "kind": "path",
            "label": { "kind": "literal", "text": { "en": "Model", "zh": "模型" } },
            "pathKind": "file"
        })
    );
}

#[test]
fn each_kind_reads_its_own_values() {
    let mut number = field("port", FieldKind::Number).bounds(1, 65535);
    number.step = Some(1.into());
    let mut slider = field("ratio", FieldKind::Slider).bounds(0, 1);
    slider.step = serde_json::Number::from_f64(0.25);
    let select = field("size", FieldKind::Select).options(vec![option("s"), option("m")]);
    let multiselect = field("tags", FieldKind::Multiselect).options(vec![option("a"), option("b")]);
    let mut catalog = field("branch", FieldKind::Select);
    catalog.options_source = Some("branches".to_string());

    for kind in [
        FieldKind::Text,
        FieldKind::Secret,
        FieldKind::Textarea,
        FieldKind::Path,
    ] {
        let text = field("value", kind);
        assert_eq!(
            text.read(&json!("x")),
            Ok(SchemaValue::Text("x".to_string()))
        );
        assert!(text.read(&json!(1)).is_err());
    }
    assert_eq!(
        number.read(&json!(8080)),
        Ok(SchemaValue::Number(8080.into()))
    );
    assert!(number.read(&json!(0)).is_err());
    assert!(number.read(&json!(80.5)).is_err());
    assert!(number.read(&json!("80")).is_err());
    assert!(slider.read(&json!(0.75)).is_ok());
    assert!(slider.read(&json!(0.3)).is_err());
    assert_eq!(
        field("on", FieldKind::Boolean).read(&json!(true)),
        Ok(SchemaValue::Boolean(true))
    );
    assert!(select.read(&json!("m")).is_ok());
    assert_eq!(
        select.read(&json!("l")).unwrap_err(),
        "`size` takes one of s, m"
    );
    assert_eq!(
        multiselect.read(&json!(["b", "a"])),
        Ok(SchemaValue::List(vec!["b".to_string(), "a".to_string()]))
    );
    assert!(multiselect.read(&json!(["c"])).is_err());
    assert!(
        catalog.read(&json!("dev")).is_ok(),
        "a host-listed select is checked where the list is"
    );
}

#[test]
fn values_take_defaults_and_refuse_undeclared_keys_only_when_strict() {
    let mut port = field("port", FieldKind::Number);
    port.default = Some(SchemaValue::Number(8080.into()));
    let fields = [port, field("name", FieldKind::Text)];

    let values = read_values(&fields, object(json!({ "name": null })), true).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values["port"], SchemaValue::Number(8080.into()));
    let values = read_values(&fields, object(json!({ "name": "" })), true).unwrap();
    assert!(
        values.get("name").is_none(),
        "empty text is no value, as a blank field is"
    );
    let mut blank = field("name", FieldKind::Text);
    blank.required = true;
    blank.default = Some(SchemaValue::Text(String::new()));
    assert_eq!(
        read_values(&[blank], Map::new(), true).unwrap_err(),
        "`name` is required",
        "an empty default is no value either"
    );

    let error = read_values(&fields, object(json!({ "old": 1 })), true).unwrap_err();
    assert_eq!(error, "undeclared fields: old");
    assert!(
        read_values(&fields, object(json!({ "old": 1 })), false)
            .unwrap()
            .get("old")
            .is_none()
    );
}

#[test]
fn stored_values_keep_what_the_form_takes_and_a_patch_reads_as_a_request() {
    let mut port = field("port", FieldKind::Number).bounds(0, 65535);
    port.default = Some(SchemaValue::Number(8080.into()));
    let fields = [port, field("name", FieldKind::Text)];

    // A value its field no longer takes, and a key no field declares, are
    // left out of what the form shows, so the field shows its default.
    let stored = stored_values(
        &fields,
        &object(json!({ "port": 70000, "name": "a", "old": 1 })),
    );
    assert_eq!(Value::Object(stored.clone()), json!({ "name": "a" }));

    let patched = patched_values(&fields, stored.clone(), object(json!({ "port": 1 }))).unwrap();
    assert_eq!(Value::Object(patched), json!({ "port": 1, "name": "a" }));
    let unset = patched_values(&fields, stored.clone(), object(json!({ "name": null }))).unwrap();
    assert_eq!(
        Value::Object(unset),
        json!({ "port": 8080 }),
        "an unset field is written as its default"
    );
    assert_eq!(
        patched_values(&fields, stored.clone(), object(json!({ "old": 1 }))).unwrap_err(),
        "undeclared fields: old"
    );
    assert_eq!(
        patched_values(&fields, stored, object(json!({ "port": 70000 }))).unwrap_err(),
        "`port` takes a number from 0 to 65535"
    );
}

#[test]
fn conditions_decide_what_is_required_and_what_is_submitted() {
    let source = field("source", FieldKind::Select).options(vec![option("url"), option("file")]);
    let mut url = field("url", FieldKind::Text);
    url.required_when = Some(SchemaCondition::equals(
        "source",
        SchemaValue::Text("url".to_string()),
    ));
    let mut file = field("file", FieldKind::Path);
    file.visible_when = Some(SchemaCondition::Any {
        any: vec![SchemaCondition::equals(
            "source",
            SchemaValue::Text("file".to_string()),
        )],
    });
    let fields = [source, url, file];

    assert_eq!(
        read_values(&fields, object(json!({ "source": "url" })), true).unwrap_err(),
        "`url` is required"
    );
    assert_eq!(
        read_values(&fields, object(json!({ "source": "url", "url": "" })), true).unwrap_err(),
        "`url` is required",
        "empty text is no value"
    );
    let values = read_values(
        &fields,
        object(json!({ "source": "url", "url": "https://x", "file": "/tmp/a" })),
        true,
    )
    .unwrap();
    assert!(
        values.get("file").is_none(),
        "a hidden field's value is dropped"
    );
    let values = read_values(
        &fields,
        object(json!({ "source": "file", "file": "/tmp/a" })),
        true,
    )
    .unwrap();
    assert_eq!(values["file"], SchemaValue::Text("/tmp/a".to_string()));

    let all = SchemaCondition::All {
        all: vec![
            SchemaCondition::equals("a", SchemaValue::Boolean(true)),
            SchemaCondition::equals("b", SchemaValue::Boolean(true)),
        ],
    };
    let mut values = super::SchemaValues::new();
    values.insert("a".to_string(), SchemaValue::Boolean(true));
    assert!(!all.matches(&values));
    values.insert("b".to_string(), SchemaValue::Boolean(true));
    assert!(all.matches(&values));
}

#[test]
fn a_condition_serializes_in_the_key_equals_any_all_form() {
    let condition = SchemaCondition::Any {
        any: vec![
            SchemaCondition::equals("autoMirror", SchemaValue::Boolean(true)),
            SchemaCondition::All {
                all: vec![SchemaCondition::equals(
                    "proxyMode",
                    SchemaValue::Text("system".to_string()),
                )],
            },
        ],
    };
    assert_eq!(
        serde_json::to_value(condition).unwrap(),
        json!({ "any": [
            { "key": "autoMirror", "equals": true },
            { "all": [{ "key": "proxyMode", "equals": "system" }] }
        ] })
    );
}

#[test]
fn a_form_from_outside_is_checked_once() {
    let mut bad_default = field("size", FieldKind::Select).options(vec![option("s")]);
    bad_default.default = Some(SchemaValue::Text("xl".to_string()));
    let mut reads_unknown = field("url", FieldKind::Text);
    reads_unknown.visible_when = Some(SchemaCondition::equals("mode", SchemaValue::Boolean(true)));
    let fields = [
        field("bad key", FieldKind::Text),
        field("dup", FieldKind::Text),
        field("dup", FieldKind::Text),
        field("empty", FieldKind::Select),
        field("range", FieldKind::Number).bounds(2, 1),
        field("slide", FieldKind::Slider),
        bad_default,
        reads_unknown,
    ];
    let issues: Vec<(usize, String)> = check_fields(&fields)
        .into_iter()
        .map(|issue| (issue.field, issue.message))
        .collect();
    assert_eq!(
        issues,
        vec![
            (0, "`bad key` is not a name of [A-Za-z0-9_-]".to_string()),
            (2, "`dup` is declared twice".to_string()),
            (3, "a select lists its options".to_string()),
            (4, "min 2 is above max 1".to_string()),
            (5, "a slider has a min and a max".to_string()),
            (6, "default: `size` takes one of s".to_string()),
            (
                7,
                "a condition reads `mode`, which is not a field".to_string()
            ),
        ]
    );
}

#[test]
fn a_field_decodes_from_what_it_serializes_to_and_ignores_what_it_does_not_know() {
    let field: SchemaField = serde_json::from_value(json!({
        "key": "threads",
        "kind": "number",
        "label": { "kind": "literal", "text": { "en": "Threads", "zh": "线程" } },
        "min": 1,
        "max": 64,
        "default": 8,
        "visibleWhen": { "key": "mode", "equals": "cpu" },
        "futureOption": true,
    }))
    .unwrap();
    assert_eq!(field.kind, FieldKind::Number);
    assert_eq!(field.default, Some(SchemaValue::Number(8.into())));
    assert_eq!(
        serde_json::to_value(&field).unwrap()["label"],
        json!({ "kind": "literal", "text": { "en": "Threads", "zh": "线程" } })
    );
}

#[test]
fn an_extension_names_only_its_own_message_keys() {
    let field = SchemaField::new(
        "model",
        FieldKind::Select,
        SchemaText::key("ext.com.example.llama.settings.model"),
    )
    .options(vec![SchemaOption {
        value: "small".to_string(),
        label: SchemaText::key("ext.com.example.llama.models.small"),
    }])
    .scoped_to("com.example.llama")
    .unwrap();
    assert_eq!(
        field.label,
        SchemaText::key("ext['com.example.llama'].settings.model")
    );
    assert_eq!(
        field.options[0].label,
        SchemaText::key("ext['com.example.llama'].models.small")
    );
    assert_eq!(
        SchemaText::literal("Model").scoped_to("com.example.llama"),
        Ok(SchemaText::literal("Model"))
    );
    for foreign in [
        "settings.fields.proxyMode.label",
        "ext.com.example.other.name",
        "ext.com.example.llamas.name",
        "ext.com.example.llama.",
    ] {
        assert_eq!(
            SchemaText::key(foreign).scoped_to("com.example.llama"),
            Err(format!(
                "message key `{foreign}` is outside `ext.com.example.llama.`"
            ))
        );
    }
}
