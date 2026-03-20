use patchworks::db::inspector::inspect_database;
use patchworks::db::snapshot::SnapshotStore;
use tempfile::TempDir;

mod support;

use support::create_db;

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
