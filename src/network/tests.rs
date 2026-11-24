use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{NetworkContext, ProxyAddress, endpoint_is_reachable, fetch_sources_with_grace};

#[test]
fn proxy_address_normalizes_socks_and_ipv6_host() {
    let proxy = ProxyAddress::parse("socks://user:secret@[::1]:1080").unwrap();

    assert_eq!(proxy.as_str(), "socks5h://user:secret@[::1]:1080");
    assert_eq!(proxy.host(), "::1");
    assert_eq!(proxy.port(), 1080);
}

#[test]
fn proxy_address_requires_supported_scheme_host_and_explicit_port() {
    for raw in [
        "",
        "127.0.0.1:7890",
        "ftp://127.0.0.1:7890",
        "http://127.0.0.1",
        "http://127.0.0.1:0",
        "http://127.0.0.1:7890/path",
    ] {
        assert!(ProxyAddress::parse(raw).is_err(), "{raw}");
    }
}

#[test]
fn connectivity_check_accepts_a_listening_endpoint_and_rejects_a_closed_one() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let listening =
        ProxyAddress::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    assert!(endpoint_is_reachable(&listening, Duration::from_secs(1)));

    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_address = closed.local_addr().unwrap();
    drop(closed);
    let closed = ProxyAddress::parse(&format!("http://{closed_address}")).unwrap();
    assert!(!endpoint_is_reachable(&closed, Duration::from_millis(200)));
}

#[test]
fn client_routes_requests_through_the_selected_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = ProxyAddress::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let context = NetworkContext { proxy: Some(proxy) };
    let client = context.client("Hanafubuki Launcher proxy test").unwrap();
    let (accepted_tx, accepted_rx) = mpsc::channel();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0_u8; 1024];
        let count = stream.read(&mut request).unwrap();
        accepted_tx.send(request[..count].to_vec()).unwrap();
        stream
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });

    let response = client
        .get("http://example.invalid/proxy-check")
        .timeout(Duration::from_secs(2))
        .send()
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let request = accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(String::from_utf8_lossy(&request).contains("example.invalid"));
    server.join().unwrap();
}

#[test]
fn source_fetches_run_together_and_drop_a_source_outlasting_the_grace() {
    let started = Instant::now();
    let results = fetch_sources_with_grace(
        vec![Duration::from_millis(50), Duration::from_secs(10)],
        |delay| {
            thread::sleep(delay);
            Ok::<_, ()>(delay)
        },
        Duration::from_millis(100),
    );

    assert_eq!(results[0], Some(Ok(Duration::from_millis(50))));
    assert_eq!(results[1], None);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_failed_source_waits_for_the_others_without_a_grace() {
    let results = fetch_sources_with_grace(
        vec![Err("offline"), Ok(Duration::from_millis(200))],
        |outcome: Result<Duration, &str>| {
            let delay = outcome?;
            thread::sleep(delay);
            Ok(delay)
        },
        Duration::ZERO,
    );

    assert_eq!(results[0], Some(Err("offline")));
    assert_eq!(results[1], Some(Ok(Duration::from_millis(200))));
}
