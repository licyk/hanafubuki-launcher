use std::collections::BTreeSet;

use super::SettingsDomain;

#[test]
fn every_domain_has_a_distinct_file_name() {
    let names: BTreeSet<&str> = SettingsDomain::ALL
        .iter()
        .map(|domain| domain.file_name())
        .collect();
    assert_eq!(
        names.len(),
        SettingsDomain::ALL.len(),
        "two domains would write to the same settings file"
    );
}

/// The files the Agent's settings used to live in belong to no domain, so
/// nothing reads or rewrites them.
#[test]
fn the_agent_files_are_no_domain_s() {
    for domain in SettingsDomain::ALL {
        assert!(!domain.file_name().starts_with("agent"), "{domain:?}");
    }
}
