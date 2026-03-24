//! Snapshot persistence for Patchworks.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use directories::BaseDirs;
use rusqlite::backup::Backup;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::inspector::inspect_database;
use crate::db::types::Snapshot;
use crate::error::{PatchworksError, Result};

/// Filesystem paths used by the snapshot store.
#[derive(Clone, Debug)]
pub struct SnapshotPaths {
    /// Root Patchworks data directory.
    pub root: PathBuf,
    /// Metadata database path.
    pub meta_db: PathBuf,
    /// Directory containing copied snapshot databases.
    pub snapshots_dir: PathBuf,
}

/// Snapshot storage backed by a metadata SQLite database.
#[derive(Clone, Debug)]
pub struct SnapshotStore {
    paths: SnapshotPaths,
}

impl SnapshotStore {
    /// Creates a snapshot store using the default Patchworks home directory.
    pub fn new_default() -> Result<Self> {
        let base_dirs = BaseDirs::new().ok_or_else(|| {
            PatchworksError::InvalidState("unable to resolve a home directory".to_owned())
        })?;
        Self::new_in(base_dirs.home_dir().join(".patchworks"))
    }

    /// Creates a snapshot store rooted at a specific path.
    pub fn new_in(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let snapshots_dir = root.join("snapshots");
        let meta_db = root.join("patchworks.db");
        fs::create_dir_all(&snapshots_dir)?;

        let store = Self {
            paths: SnapshotPaths {
                root,
                meta_db,
                snapshots_dir,
            },
        };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Returns the resolved store paths.
    pub fn paths(&self) -> &SnapshotPaths {
        &self.paths
    }

    /// Saves a snapshot of the given source database.
    pub fn save_snapshot(&self, source_path: &Path, name: &str) -> Result<Snapshot> {
        let source = source_path.canonicalize()?;
        let summary = inspect_database(&source)?;
        let snapshot_id = Uuid::new_v4().to_string();
        let snapshot_path = self
            .paths
            .snapshots_dir
            .join(format!("{snapshot_id}.sqlite"));
        copy_database_via_backup(&source, &snapshot_path)?;

        let snapshot = Snapshot {
            id: snapshot_id,
            name: name.to_owned(),
            source_path: source.to_string_lossy().into_owned(),
            created_at: Utc::now().to_rfc3339(),
            table_count: summary.tables.len() as u32,
            total_rows: summary.tables.iter().map(|table| table.row_count).sum(),
        };

        let connection = Connection::open(&self.paths.meta_db)?;
        connection.execute(
            "
            INSERT INTO snapshots (id, name, source_path, snapshot_path, created_at, table_count, total_rows)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            rusqlite::params![
                snapshot.id,
                snapshot.name,
                snapshot.source_path,
                snapshot_path.to_string_lossy().into_owned(),
                snapshot.created_at,
                snapshot.table_count,
                snapshot.total_rows as i64
            ],
        )?;

        Ok(snapshot)
    }

    /// Lists snapshots for a source database ordered newest-first.
    pub fn list_snapshots(&self, source_path: &Path) -> Result<Vec<Snapshot>> {
        let source = source_path.canonicalize()?;
        let connection = Connection::open(&self.paths.meta_db)?;
        let mut statement = connection.prepare(
            "
            SELECT id, name, source_path, created_at, table_count, total_rows
            FROM snapshots
            WHERE source_path = ?1
            ORDER BY created_at DESC
            ",
        )?;
        let rows = statement.query_map([source.to_string_lossy().as_ref()], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                name: row.get(1)?,
                source_path: row.get(2)?,
                created_at: row.get(3)?,
                table_count: row.get::<_, i64>(4)? as u32,
                total_rows: row.get::<_, i64>(5)? as u64,
            })
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    /// Returns the filesystem path for a stored snapshot.
    pub fn load_snapshot_path(&self, snapshot_id: &str) -> Result<PathBuf> {
        let connection = Connection::open(&self.paths.meta_db)?;
        let path = connection.query_row(
            "SELECT snapshot_path FROM snapshots WHERE id = ?1",
            [snapshot_id],
            |row| row.get::<_, String>(0),
        )?;
        Ok(PathBuf::from(path))
    }

    /// Lists all snapshots across all source databases, newest-first.
    pub fn list_all_snapshots(&self) -> Result<Vec<Snapshot>> {
        let connection = Connection::open(&self.paths.meta_db)?;
        let mut statement = connection.prepare(
            "
            SELECT id, name, source_path, created_at, table_count, total_rows
            FROM snapshots
            ORDER BY created_at DESC
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                name: row.get(1)?,
                source_path: row.get(2)?,
                created_at: row.get(3)?,
                table_count: row.get::<_, i64>(4)? as u32,
                total_rows: row.get::<_, i64>(5)? as u64,
            })
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    /// Deletes a snapshot by ID, removing both the metadata row and the stored database file.
    pub fn delete_snapshot(&self, snapshot_id: &str) -> Result<bool> {
        let snapshot_path = self.load_snapshot_path(snapshot_id).ok();

        let connection = Connection::open(&self.paths.meta_db)?;
        let deleted = connection.execute("DELETE FROM snapshots WHERE id = ?1", [snapshot_id])?;

        if let Some(path) = snapshot_path {
            if path.exists() {
                fs::remove_file(&path)?;
            }
        }

        Ok(deleted > 0)
    }

    fn ensure_schema(&self) -> Result<()> {
        let connection = Connection::open(&self.paths.meta_db)?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                source_path TEXT NOT NULL,
                snapshot_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                table_count INTEGER NOT NULL,
                total_rows INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_source_path_created_at
                ON snapshots (source_path, created_at DESC);
            ",
        )?;
        Ok(())
    }
}

fn copy_database_via_backup(source_path: &Path, target_path: &Path) -> Result<()> {
    let source = Connection::open(source_path)?;
    let mut target = Connection::open(target_path)?;
    let backup = Backup::new(&source, &mut target)?;
    backup.step(-1)?;
    Ok(())
}
