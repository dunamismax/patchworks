//! Recent-files persistence under `~/.patchworks/recent.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const MAX_RECENT_FILES: usize = 20;
const RECENT_FILENAME: &str = "recent.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentStore {
    files: Vec<PathBuf>,
}

/// Returns the path to the recent-files JSON store.
fn recent_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "patchworks")
        .map(|dirs| dirs.data_dir().join(RECENT_FILENAME))
}

/// Loads the recent-files list from disk.
pub fn load_recent_files() -> Vec<PathBuf> {
    let Some(path) = recent_path() else {
        return Vec::new();
    };
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str::<RecentStore>(&data)
        .map(|store| store.files)
        .unwrap_or_default()
}

/// Records a file path at the top of the recent-files list and persists to disk.
pub fn push_recent_file(path: &Path) {
    let Some(store_path) = recent_path() else {
        return;
    };
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut files = load_recent_files();
    files.retain(|existing| {
        let existing_canonical = fs::canonicalize(existing).unwrap_or_else(|_| existing.clone());
        existing_canonical != canonical
    });
    files.insert(0, canonical);
    files.truncate(MAX_RECENT_FILES);

    let store = RecentStore { files };
    if let Some(parent) = store_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(
        &store_path,
        serde_json::to_string_pretty(&store).unwrap_or_default(),
    );
}
