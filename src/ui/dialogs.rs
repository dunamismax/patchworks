//! Small wrappers around native dialogs.

use std::path::PathBuf;

/// Opens a native file dialog filtered to common SQLite extensions.
pub fn open_database_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("SQLite Database", &["db", "sqlite", "sqlite3"])
        .pick_file()
}

/// Opens a native save dialog for SQL export files.
pub fn save_sql_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("SQL Script", &["sql"])
        .set_file_name("patchworks-diff.sql")
        .save_file()
}
