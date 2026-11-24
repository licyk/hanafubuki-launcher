use super::parse_proxy_server;

#[test]
fn parses_a_bare_windows_proxy_server() {
    assert_eq!(
        parse_proxy_server("127.0.0.1:7890"),
        Some("http://127.0.0.1:7890".to_string())
    );
}

#[test]
fn prefers_the_https_mapping_for_launcher_traffic() {
    assert_eq!(
        parse_proxy_server("http=127.0.0.1:8080;https=127.0.0.1:8443;socks=127.0.0.1:1080"),
        Some("http://127.0.0.1:8443".to_string())
    );
}

#[test]
fn parses_a_socks_only_windows_proxy_server() {
    assert_eq!(
        parse_proxy_server("socks=socks://127.0.0.1:1080"),
        Some("socks://127.0.0.1:1080".to_string())
    );
}
