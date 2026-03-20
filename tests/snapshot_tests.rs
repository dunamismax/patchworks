use std::collections::HashMap;
use std::path::PathBuf;

use patchworks::db::inspector::inspect_database;
use patchworks::db::snapshot::SnapshotStore;
use rusqlite::Connection;
use tempfile::TempDir;

fn fixture_sql(name: &str) -> String {
    let content = include_str!("fixtures/create_fixtures.sql");
    let mut fixtures = HashMap::new();
    let mut current_name = None::<String>;
    let mut buffer = String::new();

    for line in content.lines() {
        if let Some(name) = line.strip_prefix("-- @fixture ") {
            if let Some(previous) = current_name.replace(name.trim().to_owned()) {
                fixtures.insert(previous, buffer.trim().to_owned());
                buffer.clear();
            }
        } else {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    if let Some(previous) = current_name {
        fixtures.insert(previous, buffer.trim().to_owned());
    }

    fixtures.get(name).cloned().expect("fixture exists")
}

fn create_db(dir: &TempDir, file_name: &str, fixture_name: &str) -> PathBuf {
    let path = dir.path().join(file_name);
    let connection = Connection::open(&path).expect("create sqlite db");
    connection
        .execute_batch(&fixture_sql(fixture_name))
        .expect("apply fixture sql");
    path
}

#[test]
fn snapshot_round_trip_preserves_database_contents() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = create_db(&temp_dir, "source.sqlite", "snapshot_source");
    let store =
        SnapshotStore::new_in(temp_dir.path().join(".patchworks-test")).expect("create store");

    let saved = store
        .save_snapshot(&db_path, "seed state")
        .expect("save snapshot");
    let listed = store.list_snapshots(&db_path).expect("list snapshots");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, saved.id);

    let snapshot_path = store.load_snapshot_path(&saved.id).expect("load snapshot");
    let source_summary = inspect_database(&db_path).expect("inspect source");
    let snapshot_summary = inspect_database(&snapshot_path).expect("inspect snapshot");

    assert_eq!(source_summary.tables, snapshot_summary.tables);
}
