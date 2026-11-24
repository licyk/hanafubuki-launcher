//! Descriptive metadata shared by configuration validators and schema exports.
//! These records document existing checks; they do not execute predicates.
use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleStage {
    Validation,
    Package,
    Compatibility,
    Catalog,
}

#[derive(Clone, Copy, Serialize)]
pub struct ConfigRule {
    pub id: &'static str,
    pub fields: &'static [&'static str],
    pub condition: &'static str,
    pub requirement: &'static str,
    #[serde(skip)]
    pub message: &'static str,
    pub stage: RuleStage,
    pub source: &'static str,
}

#[cfg(feature = "schema")]
pub fn annotate(schema: &mut schemars::Schema, rules: &[ConfigRule]) {
    schema.insert(
        "x-hanafubuki-rules".into(),
        serde_json::to_value(rules).expect("static configuration rules serialize"),
    );
}
