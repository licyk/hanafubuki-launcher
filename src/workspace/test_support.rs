use crate::internal::hanafubuki_foundation::paths::ResolvedPath;

pub(super) fn tempdir() -> tempfile::TempDir {
    // System temporary paths may traverse a symlink (macOS /var).
    let parent = ResolvedPath::resolve(&std::env::temp_dir()).unwrap();
    tempfile::tempdir_in(parent.system()).unwrap()
}
