use crate::internal::hanafubuki_foundation::config_documentation::{ConfigRule, RuleStage};
pub(super) const SECRET: ConfigRule = ConfigRule {
    id: "preset-secret",
    fields: &["install"],
    condition: "Preset values are checked against the currently registered base type form",
    requirement: "Preset install defaults must not include secret fields.",
    message: "is a secret, which a preset never holds",
    stage: RuleStage::Catalog,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/presets/mod.rs",
};
pub(super) const LAUNCH: ConfigRule = ConfigRule {
    id: "preset-launch-capability",
    fields: &["base", "launch.arguments"],
    condition: "Launch arguments are nonempty",
    requirement: "Nonempty launch arguments require the base type to expose launch-arguments.",
    message: "takes no launch arguments",
    stage: RuleStage::Catalog,
    source: "src-tauri/crates/hanafubuki-webui/src/instance_types/presets/mod.rs",
};
