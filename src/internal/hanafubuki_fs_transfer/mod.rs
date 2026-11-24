//! Shared, Tauri-free filesystem transfer mechanisms.
//!
//! Business domains retain ownership of path policy, journals, cancellation
//! and publication. This crate owns safe byte copying, no-replace moves and
//! metadata application order.

mod copy;
mod metadata;
mod move_path;
mod symlink;

pub use copy::copy_file_create_new;
pub use metadata::{preserve_path_metadata, preserve_symlink_times, remove_dir_all};
pub use move_path::{
    InitialMoveOutcome, attempt_no_replace_move, is_cross_device_error, move_no_replace,
};
pub use symlink::{copy_symlink, reject_unsupported_reparse};
