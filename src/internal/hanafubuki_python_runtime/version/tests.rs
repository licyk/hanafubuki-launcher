use super::*;

#[test]
fn every_version_round_trips_through_its_name() {
    for version in PythonVersion::ALL {
        assert_eq!(PythonVersion::try_from(version.as_str()), Ok(version));
    }
    for unsupported in ["3.9", "3.16", " 3.11", "3", "", "banana"] {
        assert!(PythonVersion::try_from(unsupported).is_err());
    }
}

#[test]
fn matches_patch_releases_of_the_same_feature_release() {
    assert!(PythonVersion::Python311.matches_release("3.11.14"));
    assert!(PythonVersion::Python315.matches_release("3.15.0"));
    assert!(!PythonVersion::Python311.matches_release("3.12.0"));
    assert!(!PythonVersion::Python311.matches_release("3.110.0"));
    assert!(!PythonVersion::Python311.matches_release("not-a-version"));
}
