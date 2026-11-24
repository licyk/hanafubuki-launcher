#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileProgress {
    pub completed: u64,
    pub total: Option<u64>,
    pub current_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub version: String,
    pub completed: u64,
    pub total: Option<u64>,
}

pub mod handoff;
pub mod manifest;
pub mod package;
mod package_signature;
pub mod release;
