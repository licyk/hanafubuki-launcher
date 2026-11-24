use super::{host_port_proxy, parse_kde_config};

#[test]
fn parses_gnome_host_and_port_values() {
    assert_eq!(
        host_port_proxy("http", "'127.0.0.1'\n", "7890\n"),
        Some("http://127.0.0.1:7890".to_string())
    );
    assert_eq!(host_port_proxy("http", "''", "0"), None);
}

#[test]
fn parses_manual_kde_proxy_and_prefers_https_mapping() {
    let content = r#"
[Proxy Settings]
ProxyType=1
httpProxy=http://127.0.0.1 8080
httpsProxy=http://127.0.0.1 8443
socksProxy=socks://127.0.0.1 1080
"#;

    assert_eq!(
        parse_kde_config(content),
        Some("http://127.0.0.1:8443".to_string())
    );
}

#[test]
fn ignores_non_manual_or_malformed_kde_proxy() {
    assert_eq!(
        parse_kde_config("[Proxy Settings]\nProxyType=0\nhttpProxy=127.0.0.1 8080"),
        None
    );
    assert_eq!(
        parse_kde_config("[Proxy Settings]\nProxyType=1\nhttpProxy=too many values here"),
        None
    );
}
