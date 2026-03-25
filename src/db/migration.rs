//! Migration chain persistence for Patchworks.
//!
//! Stores, retrieves, and manages ordered migration sequences in the Patchworks
//! local store alongside snapshot data.

use std::collections::BTreeSet;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::snapshot::SnapshotStore;
use crate::db::types::{Migration, MigrationChainSummary, MigrationConflict};
use crate::error::Result;

/// Parameters for creating a new migration.
pub struct NewMigration<'a> {
    /// Human-readable migration name.
    pub name: &'a str,
    /// SQL statements that apply the forward migration.
    pub up_sql: &'a str,
    /// SQL statements that reverse the migration, if reversible.
    pub down_sql: Option<&'a str>,
    /// Source database path at migration creation time.
    pub source_path: &'a str,
    /// Target database path at migration creation time.
    pub target_path: &'a str,
    /// Schema objects affected by this migration.
    pub affected_tables: &'a [String],
    /// Optional description of what this migration does.
    pub description: Option<&'a str>,
}

/// Migration storage backed by the Patchworks metadata SQLite database.
///
/// Reuses the same `~/.patchworks/patchworks.db` that snapshots live in,
/// adding a `migrations` table for chain management.
#[derive(Clone, Debug)]
pub struct MigrationStore {
    store: SnapshotStore,
}

impl MigrationStore {
    /// Creates a migration store using the default Patchworks home directory.
    pub fn new_default() -> Result<Self> {
        let store = SnapshotStore::new_default()?;
        let s = Self { store };
        s.ensure_migration_schema()?;
        Ok(s)
    }

    /// Creates a migration store rooted at a specific path.
    pub fn new_in(root: impl AsRef<Path>) -> Result<Self> {
        let store = SnapshotStore::new_in(root)?;
        let s = Self { store };
        s.ensure_migration_schema()?;
        Ok(s)
    }

    /// Returns a reference to the underlying snapshot store.
    pub fn snapshot_store(&self) -> &SnapshotStore {
        &self.store
    }

    /// Saves a new migration to the store with the next sequence number.
    pub fn save_migration(&self, params: &NewMigration<'_>) -> Result<Migration> {
        let connection = self.open_meta_db()?;
        let next_sequence = self.next_sequence(&connection)?;
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let tables_json = serde_json::to_string(params.affected_tables)?;

        connection.execute(
            "
            INSERT INTO migrations (id, name, up_sql, down_sql, source_path, target_path,
                                    sequence, created_at, validated, affected_tables, description)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            rusqlite::params![
                id,
                params.name,
                params.up_sql,
                params.down_sql,
                params.source_path,
                params.target_path,
                next_sequence,
                created_at,
                false,
                tables_json,
                params.description,
            ],
        )?;

        Ok(Migration {
            id,
            name: params.name.to_owned(),
            up_sql: params.up_sql.to_owned(),
            down_sql: params.down_sql.map(|s| s.to_owned()),
            source_path: params.source_path.to_owned(),
            target_path: params.target_path.to_owned(),
            sequence: next_sequence,
            created_at,
            validated: false,
            affected_tables: params.affected_tables.to_vec(),
            description: params.description.map(|s| s.to_owned()),
        })
    }

    /// Lists all migrations ordered by sequence number.
    pub fn list_migrations(&self) -> Result<Vec<Migration>> {
        let connection = self.open_meta_db()?;
        let mut statement = connection.prepare(
            "
            SELECT id, name, up_sql, down_sql, source_path, target_path,
                   sequence, created_at, validated, affected_tables, description
            FROM migrations
            ORDER BY sequence ASC
            ",
        )?;

        let rows = statement.query_map([], |row| {
            let tables_json: String = row.get(9)?;
            let affected_tables: Vec<String> =
                serde_json::from_str(&tables_json).unwrap_or_default();
            Ok(Migration {
                id: row.get(0)?,
                name: row.get(1)?,
                up_sql: row.get(2)?,
                down_sql: row.get(3)?,
                source_path: row.get(4)?,
                target_path: row.get(5)?,
                sequence: row.get::<_, i64>(6)? as u32,
                created_at: row.get(7)?,
                validated: row.get(8)?,
                affected_tables,
                description: row.get(10)?,
            })
        })?;

        let mut migrations = Vec::new();
        for row in rows {
            migrations.push(row?);
        }
        Ok(migrations)
    }

    /// Retrieves a single migration by ID.
    pub fn get_migration(&self, migration_id: &str) -> Result<Migration> {
        let connection = self.open_meta_db()?;
        let migration = connection.query_row(
            "
            SELECT id, name, up_sql, down_sql, source_path, target_path,
                   sequence, created_at, validated, affected_tables, description
            FROM migrations
            WHERE id = ?1
            ",
            [migration_id],
            |row| {
                let tables_json: String = row.get(9)?;
                let affected_tables: Vec<String> =
                    serde_json::from_str(&tables_json).unwrap_or_default();
                Ok(Migration {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    up_sql: row.get(2)?,
                    down_sql: row.get(3)?,
                    source_path: row.get(4)?,
                    target_path: row.get(5)?,
                    sequence: row.get::<_, i64>(6)? as u32,
                    created_at: row.get(7)?,
                    validated: row.get(8)?,
                    affected_tables,
                    description: row.get(10)?,
                })
            },
        )?;
        Ok(migration)
    }

    /// Marks a migration as validated.
    pub fn mark_validated(&self, migration_id: &str) -> Result<bool> {
        let connection = self.open_meta_db()?;
        let updated = connection.execute(
            "UPDATE migrations SET validated = 1 WHERE id = ?1",
            [migration_id],
        )?;
        Ok(updated > 0)
    }

    /// Deletes a migration by ID.
    pub fn delete_migration(&self, migration_id: &str) -> Result<bool> {
        let connection = self.open_meta_db()?;
        let deleted = connection.execute("DELETE FROM migrations WHERE id = ?1", [migration_id])?;
        Ok(deleted > 0)
    }

    /// Computes a summary of the current migration chain.
    pub fn chain_summary(&self) -> Result<MigrationChainSummary> {
        let migrations = self.list_migrations()?;
        if migrations.is_empty() {
            return Ok(MigrationChainSummary::default());
        }

        let mut all_tables = BTreeSet::new();
        let mut validated_count = 0;
        let mut reversible_count = 0;

        for migration in &migrations {
            for table in &migration.affected_tables {
                all_tables.insert(table.clone());
            }
            if migration.validated {
                validated_count += 1;
            }
            if migration.down_sql.is_some() {
                reversible_count += 1;
            }
        }

        Ok(MigrationChainSummary {
            total_migrations: migrations.len(),
            validated_count,
            reversible_count,
            first_sequence: migrations.first().map(|m| m.sequence),
            last_sequence: migrations.last().map(|m| m.sequence),
            all_affected_tables: all_tables.into_iter().collect(),
        })
    }

    /// Detects conflicts between migrations that target overlapping schema objects.
    pub fn detect_conflicts(&self) -> Result<Vec<MigrationConflict>> {
        let migrations = self.list_migrations()?;
        let mut conflicts = Vec::new();

        for i in 0..migrations.len() {
            for j in (i + 1)..migrations.len() {
                let a = &migrations[i];
                let b = &migrations[j];
                let a_tables: BTreeSet<&str> =
                    a.affected_tables.iter().map(|s| s.as_str()).collect();
                let b_tables: BTreeSet<&str> =
                    b.affected_tables.iter().map(|s| s.as_str()).collect();
                let overlap: Vec<String> = a_tables
                    .intersection(&b_tables)
                    .map(|s| s.to_string())
                    .collect();
                if !overlap.is_empty() {
                    conflicts.push(MigrationConflict {
                        migration_a_id: a.id.clone(),
                        migration_b_id: b.id.clone(),
                        overlapping_tables: overlap.clone(),
                        description: format!(
                            "Migrations '{}' (seq {}) and '{}' (seq {}) both modify: {}",
                            a.name,
                            a.sequence,
                            b.name,
                            b.sequence,
                            overlap.join(", ")
                        ),
                    });
                }
            }
        }

        Ok(conflicts)
    }

    fn open_meta_db(&self) -> Result<Connection> {
        Ok(Connection::open(&self.store.paths().meta_db)?)
    }

    fn next_sequence(&self, connection: &Connection) -> Result<u32> {
        let max: Option<i64> =
            connection.query_row("SELECT MAX(sequence) FROM migrations", [], |row| row.get(0))?;
        Ok(max.map(|n| n as u32 + 1).unwrap_or(1))
    }

    fn ensure_migration_schema(&self) -> Result<()> {
        let connection = self.open_meta_db()?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS migrations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                up_sql TEXT NOT NULL,
                down_sql TEXT,
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                validated INTEGER NOT NULL DEFAULT 0,
                affected_tables TEXT NOT NULL DEFAULT '[]',
                description TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_migrations_sequence
                ON migrations (sequence ASC);
            ",
        )?;
        Ok(())
    }
}
