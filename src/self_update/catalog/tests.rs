use super::*;

const TARGETS: [(&str, &str); 5] = [
    ("windows", "x86_64"),
    ("macos", "aarch64"),
    ("macos", "x86_64"),
    ("linux", "aarch64"),
    ("linux", "x86_64"),
];

fn document(source: Source, version: &str) -> Vec<u8> {
    let mut platforms = serde_json::Map::new();
    for (platform, architecture) in TARGETS {
        let architectures = platforms
            .entry(platform)
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap();
        architectures.insert(
            architecture.to_string(),
            serde_json::json!({
                "url": match source.provider {
                    Provider::Modelscope => format!(
                        "https://www.modelscope.cn/api/v1/models/owner/repo/repo?Revision=master&FilePath=releases%2F{version}%2F{platform}%2F{architecture}%2Fartifact"
                    ),
                    Provider::Huggingface => format!(
                        "https://huggingface.co/owner/repo/resolve/main/releases/{version}/{platform}/{architecture}/artifact"
                    ),
                },
                "signature": "c2lnbmF0dXJl"
            }),
        );
    }
    serde_json::to_vec(&serde_json::json!({
        "version": version,
        "platforms": platforms,
    }))
    .unwrap()
}

#[test]
fn accepts_the_exact_signed_provider_catalog() {
    let candidate = decode(SOURCES[1], &document(SOURCES[1], "1.2.3")).unwrap();
    assert_eq!(candidate.version, Version::new(1, 2, 3));
    assert_eq!(candidate.provider, Provider::Huggingface);
}

#[test]
fn rejects_an_unsigned_own_target_but_accepts_extra_fields() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["platforms"]["windows"]["x86_64"]["signature"] = serde_json::json!("");
    assert!(decode_windows(&raw).is_err());

    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["notes"] = serde_json::json!("Release notes");
    raw["pub_date"] = serde_json::json!("2026-11-06T00:00:00Z");
    raw["platforms"]["windows"]["x86_64"]["size"] = serde_json::json!(42);
    let candidate = decode(SOURCES[0], &serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(candidate.version, Version::new(1, 2, 3));
    assert_eq!(decode_windows(&raw).unwrap().signature, "c2lnbmF0dXJl");
}

fn decode_windows(raw: &serde_json::Value) -> Result<Candidate, String> {
    let raw: RawCatalog = serde_json::from_value(raw.clone()).unwrap();
    let version = Version::parse(&raw.version).unwrap();
    decode_for_target(SOURCES[0], raw, version, ("windows", "x86_64"))
}

#[test]
fn accepts_a_catalog_missing_only_other_targets() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    let platforms = raw["platforms"].as_object_mut().unwrap();
    platforms.remove("linux");
    platforms.remove("macos");
    let candidate = decode_windows(&raw).unwrap();
    assert!(candidate.url.contains("windows%2Fx86_64"));

    // Another target's signature is not the current target's concern.
    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["platforms"]["linux"]["x86_64"]["signature"] = serde_json::json!("");
    assert!(decode_windows(&raw).is_ok());
}

#[test]
fn rejects_a_missing_or_unreadable_own_target_and_ignores_unknown_targets() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["platforms"].as_object_mut().unwrap().remove("windows");
    assert!(decode_windows(&raw).is_err());

    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["platforms"]["windows"]["x86_64"] = serde_json::json!({"url": "https://example.test"});
    assert!(decode_windows(&raw).is_err());

    // Targets this build does not know, in any shape, are not its concern.
    let mut raw: serde_json::Value =
        serde_json::from_slice(&document(SOURCES[0], "1.2.3")).unwrap();
    raw["platforms"]["windows"]["aarch64"] = serde_json::json!({"signature": ""});
    raw["platforms"]["freebsd"] = serde_json::json!({"x86_64": {"url": 7}});
    raw["platforms"]["android"] = serde_json::json!("soon");
    let candidate = decode_windows(&raw).unwrap();
    assert!(candidate.url.contains("windows%2Fx86_64"));
}

#[test]
fn accepts_artifact_urls_without_provider_path_validation() {
    let candidate = decode(SOURCES[0], &document(SOURCES[0], "1.2.3")).unwrap();
    assert!(
        candidate
            .url
            .starts_with("https://www.modelscope.cn/api/v1/")
    );
}

#[test]
fn reconciliation_requires_identical_version_and_signature() {
    let first = decode(SOURCES[0], &document(SOURCES[0], "1.2.3")).unwrap();
    let second = decode(SOURCES[1], &document(SOURCES[1], "1.2.3")).unwrap();
    assert_eq!(
        reconcile(vec![first.clone(), second.clone()])
            .unwrap()
            .len(),
        2
    );

    let newer = decode(SOURCES[1], &document(SOURCES[1], "1.2.4")).unwrap();
    assert!(reconcile(vec![first.clone(), newer]).is_none());

    let mut different_signature = second;
    different_signature.signature = "ZGlmZmVyZW50".to_string();
    assert!(reconcile(vec![first, different_signature]).is_none());
}
