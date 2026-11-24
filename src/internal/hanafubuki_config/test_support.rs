use crate::internal::hanafubuki_foundation::paths::ResolvedPath;
use std::path::PathBuf;

pub(crate) fn temp_root() -> PathBuf {
    // A system temp path may contain symlink ancestors (such as macOS /var).
    ResolvedPath::resolve(&std::env::temp_dir())
        .unwrap()
        .native()
        .to_path_buf()
}

pub(crate) fn tempdir() -> tempfile::TempDir {
    let parent = ResolvedPath::resolve(&std::env::temp_dir()).unwrap();
    tempfile::tempdir_in(parent.system()).unwrap()
}
