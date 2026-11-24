use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::*;

#[test]
fn reads_the_updater_public_key_from_the_tauri_configuration() {
    assert_eq!(
        embedded_public_key().unwrap(),
        "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU0MDlBOTgzN0UzMkJFMTQKUldRVXZqSitnNmtKNVBkVUNEOGI5Ly90dHZZdkhhL2pWNlQ1VFVMT3hOSzBPa3FZMXJuWTRiQ3QK"
    );
    UpdaterKey::embedded().unwrap();
}

#[test]
fn verifies_the_same_base64_wrapped_minisign_contract_as_tauri_updater() {
    let public_key = "untrusted comment: minisign public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    let signature = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";
    let key = UpdaterKey::decode(&STANDARD.encode(public_key)).unwrap();
    let signature = ReleaseSignature::decode(&STANDARD.encode(signature)).unwrap();

    key.verify(b"test", &signature).unwrap();
    assert!(key.verify(b"tampered", &signature).is_err());
}

#[test]
fn a_signature_that_is_not_wrapped_minisign_is_refused() {
    assert!(ReleaseSignature::decode("not base64!").is_err());
    assert!(ReleaseSignature::decode(&STANDARD.encode("not minisign")).is_err());
    assert!(UpdaterKey::decode(&STANDARD.encode("not a key")).is_err());
}
