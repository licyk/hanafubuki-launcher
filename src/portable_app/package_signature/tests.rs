use crate::internal::hanafubuki_app_update::{AppArchitecture, AppPlatform};

use super::*;

/// A throwaway key and a signature over [`PAYLOAD`], both written by
/// `tauri signer` (the signer the release pipeline uses), so they carry its
/// prehashed format. The secret key was not kept.
const TEST_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc5N0EzMzkzQTMwQUJBRDcKUldUWHVncWprek42ZVNFOEJyZURETlBVTE9RVHQweUs4UHdmZHVOQ2lGb2dEaVVZMHFqdnUzWmgK";
const PAYLOAD: &[u8] = b"Hanafubuki portable package fixture\n";
const PAYLOAD_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUWHVncWprek42ZWRGR3lLaWJhWk9hak5yMmNoa1lCb29mc3AwWGpZVEJBVXc5OTFIY0F1blpOS2hrV3lOZHltUnpXVHVhN0lVN3NiRlJ4cXErenZDMnNlanVFVkszWFE0PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MDI4NjExCWZpbGU6cGF5bG9hZC5iaW4KUTBTTnB2RzRPaTRZUWJ3VHc0bW9idnM4WnA0NW52UUJXaDNUejcyZFdtaC81Qm44S3M1ZnhiK0lpaTdwZGo2OUZWYU8vUkR0bk5BVEwxbEZ2c1IxQ1E9PQo=";

fn check_streamed(
    key: &UpdaterKey,
    signature: &ReleaseSignature,
    data: &[u8],
) -> Result<bool, String> {
    let mut check = PackageCheck::new(key, Some(signature))?;
    for chunk in data.chunks(7) {
        check.update(chunk);
    }
    check.finish()
}

fn artifact(kind: &str, file_name: &str, url: &str) -> ValidatedCatalogArtifact {
    ValidatedCatalogArtifact {
        platform: AppPlatform::Windows,
        architecture: AppArchitecture::X86_64,
        kind: kind.into(),
        file_name: file_name.into(),
        size: 1,
        url: url.into(),
    }
}

#[test]
fn a_package_matching_its_signature_is_accepted() {
    let key = UpdaterKey::decode(TEST_PUBLIC_KEY).unwrap();
    // A `.sig` file read from the mirror may end with a newline.
    let signature = ReleaseSignature::decode(&format!("{PAYLOAD_SIGNATURE}\n")).unwrap();
    assert_eq!(check_streamed(&key, &signature, PAYLOAD), Ok(true));
}

#[test]
fn a_tampered_or_truncated_package_is_rejected() {
    let key = UpdaterKey::decode(TEST_PUBLIC_KEY).unwrap();
    let signature = ReleaseSignature::decode(PAYLOAD_SIGNATURE).unwrap();
    let mut tampered = PAYLOAD.to_vec();
    tampered[3] ^= 1;
    assert!(check_streamed(&key, &signature, &tampered).is_err());
    assert!(check_streamed(&key, &signature, &PAYLOAD[..PAYLOAD.len() - 1]).is_err());
}

#[test]
fn a_signature_from_another_key_is_rejected() {
    let key = UpdaterKey::embedded().unwrap();
    let signature = ReleaseSignature::decode(PAYLOAD_SIGNATURE).unwrap();
    assert!(PackageCheck::new(&key, Some(&signature)).is_err());
}

#[test]
fn an_unsigned_check_reports_size_only() {
    let key = UpdaterKey::decode(TEST_PUBLIC_KEY).unwrap();
    let mut check = PackageCheck::new(&key, None).unwrap();
    check.update(PAYLOAD);
    assert_eq!(check.finish(), Ok(false));
}

#[test]
fn only_a_portable_zip_before_the_first_signed_release_may_be_unsigned() {
    let before = Version::new(0, 3, 28);
    assert!(!signature_required("portable-zip", &before));
    assert!(signature_required(
        "portable-zip",
        &SIGNED_PORTABLE_ZIP_SINCE
    ));
    assert!(signature_required("portable-zip", &Version::new(1, 0, 0)));
    // The updater kinds were signed in every release.
    assert!(signature_required("appimage", &Version::new(0, 1, 0)));
    assert!(signature_required("app-tar-gz", &Version::new(0, 1, 0)));
}

#[test]
fn a_missing_signature_follows_the_transition_rule() {
    let url = "https://example.invalid/package.sig";
    assert!(matches!(
        missing("portable-zip", &Version::new(0, 3, 28), url),
        Ok(None)
    ));
    assert!(matches!(
        missing("portable-zip", &SIGNED_PORTABLE_ZIP_SINCE, url),
        Err(RemoteError::Fatal(message)) if message.contains("no published signature")
    ));
    assert!(matches!(
        missing("appimage", &Version::new(0, 3, 28), url),
        Err(RemoteError::Fatal(_))
    ));
}

#[test]
fn the_signature_url_is_derived_in_every_catalog_url_form() {
    let name = "Hanafubuki-0.3.29-windows-x86_64-portable.zip";
    let path = format!("hanafubuki/releases/0.3.29/windows/x86_64/{name}");
    let huggingface =
        format!("https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/{path}");
    assert_eq!(
        signature_url(&artifact("portable-zip", name, &huggingface)).unwrap(),
        format!("{huggingface}.sig")
    );
    let resolve =
        format!("https://modelscope.cn/models/licyks/sd-webui-all-in-one/resolve/master/{path}");
    assert_eq!(
        signature_url(&artifact("portable-zip", name, &resolve)).unwrap(),
        format!("{resolve}.sig")
    );
    let encoded = path.replace('/', "%2F");
    let api = "https://www.modelscope.cn/api/v1/models/licyks/sd-webui-all-in-one/repo";
    assert_eq!(
        signature_url(&artifact(
            "portable-zip",
            name,
            &format!("{api}?Revision=master&FilePath={encoded}")
        ))
        .unwrap(),
        format!("{api}?Revision=master&FilePath={encoded}.sig")
    );
    assert_eq!(
        signature_url(&artifact(
            "portable-zip",
            name,
            &format!("{api}?FilePath={encoded}&Revision=master")
        ))
        .unwrap(),
        format!("{api}?FilePath={encoded}.sig&Revision=master")
    );
}

#[test]
fn a_url_that_does_not_name_the_package_has_no_signature_url() {
    let name = "Hanafubuki-0.3.29-windows-x86_64-portable.zip";
    for url in [
        "https://huggingface.co/licyk/sd-webui-all-in-one/resolve/main/other.zip",
        "https://huggingface.co/x/resolve/main/Hanafubuki-0.3.29-windows-x86_64-portable.zip?download=1",
        "https://www.modelscope.cn/api/v1/models/x/repo?Revision=master&FilePath=other.zip",
    ] {
        assert!(
            signature_url(&artifact("portable-zip", name, url)).is_err(),
            "{url}"
        );
    }
}
