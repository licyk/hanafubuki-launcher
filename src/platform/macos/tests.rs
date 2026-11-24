use super::parse_scutil;

#[test]
fn parses_scutil_and_prefers_the_https_mapping() {
    let output = r#"
<dictionary> {
  HTTPEnable : 1
  HTTPProxy : 127.0.0.1
  HTTPPort : 8080
  HTTPSEnable : 1
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 8443
}
"#;

    assert_eq!(
        parse_scutil(output),
        Some("http://127.0.0.1:8443".to_string())
    );
}

#[test]
fn parses_enabled_socks_and_ignores_disabled_entries() {
    let output = r#"
<dictionary> {
  HTTPEnable : 0
  HTTPProxy : 127.0.0.1
  HTTPPort : 8080
  SOCKSEnable : 1
  SOCKSProxy : 127.0.0.1
  SOCKSPort : 1080
}
"#;

    assert_eq!(
        parse_scutil(output),
        Some("socks://127.0.0.1:1080".to_string())
    );
}
