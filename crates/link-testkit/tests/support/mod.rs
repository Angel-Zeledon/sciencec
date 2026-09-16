//! Scratch directories and file helpers shared by the harness tests.
//!
//! Deliberately dependency free: the crate has no dev-dependencies, so a
//! temporary directory is built by hand from the process id and a counter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, empty directory under the system temp directory.
///
/// `label` only makes the path readable when a test leaves one behind; the
/// uniqueness comes from the process id and the counter.
pub fn scratch(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir()
        .join("link-testkit")
        .join(format!("{}-{}-{}", std::process::id(), n, label));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir
}

/// Write `contents` to `path` verbatim, creating parent directories.
pub fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("could not create a parent directory");
    }
    std::fs::write(path, contents).expect("could not write the file");
}

/// Read `path` verbatim, without normalising anything.
pub fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path.as_ref()).expect("could not read the file")
}
