"""Migration chain persistence and conflict detection.

Stores migration metadata in ``~/.patchworks/patchworks.db`` alongside
the snapshot store.  Each migration has a unique ID, ordering sequence,
forward SQL, reverse SQL, and metadata about the source/target states.
"""

from __future__ import annotations

import sqlite3
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path


@dataclass(frozen=True)
class MigrationInfo:
    """Metadata for a stored migration."""

    id: str
    """UUID identifying the migration."""
    name: str | None
    """Optional human-readable label."""
    sequence: int
    """Monotonically increasing ordering number."""
    source_path: str
    """Resolved path of the base (left) database at generation time."""
    target_path: str
    """Resolved path of the target (right) database at generation time."""
    forward_sql: str
    """SQL that transforms source into target."""
    reverse_sql: str
    """SQL that reverts target back to source."""
    objects_touched: str
    """Comma-separated list of database objects modified by this migration."""
    created_at: str
    """ISO-8601 timestamp in UTC."""
    applied: bool
    """Whether this migration has been applied."""
    applied_at: str | None
    """ISO-8601 timestamp of application, or ``None``."""


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------

_CREATE_MIGRATIONS_TABLE = """\
CREATE TABLE IF NOT EXISTS migrations (
    id              TEXT PRIMARY KEY,
    name            TEXT,
    sequence        INTEGER NOT NULL UNIQUE,
    source_path     TEXT NOT NULL,
    target_path     TEXT NOT NULL,
    forward_sql     TEXT NOT NULL,
    reverse_sql     TEXT NOT NULL,
    objects_touched TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    applied         INTEGER NOT NULL DEFAULT 0,
    applied_at      TEXT
)
"""


# ---------------------------------------------------------------------------
# Store
# ---------------------------------------------------------------------------


class MigrationStore:
    """Manage migration metadata under ``~/.patchworks/``."""

    def __init__(self, base_dir: str | Path | None = None) -> None:
        if base_dir is None:
            self._base = Path.home() / ".patchworks"
        else:
            self._base = Path(base_dir)
        self._db_path = self._base / "patchworks.db"

        # Ensure directory exists.
        self._base.mkdir(parents=True, exist_ok=True)

        # Ensure the migrations table exists.
        conn = self._connect()
        try:
            conn.execute(_CREATE_MIGRATIONS_TABLE)
            conn.commit()
        finally:
            conn.close()

    # -- public API ---------------------------------------------------------

    def save(
        self,
        *,
        source_path: str,
        target_path: str,
        forward_sql: str,
        reverse_sql: str,
        objects_touched: list[str] | None = None,
        name: str | None = None,
    ) -> MigrationInfo:
        """Record a new migration and return its metadata."""
        mig_id = str(uuid.uuid4())
        created = datetime.now(UTC).isoformat()
        seq = self._next_sequence()
        objs = ",".join(sorted(objects_touched or []))

        conn = self._connect()
        try:
            conn.execute(
                "INSERT INTO migrations "
                "(id, name, sequence, source_path, target_path, "
                "forward_sql, reverse_sql, objects_touched, created_at, applied) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
                (
                    mig_id,
                    name,
                    seq,
                    source_path,
                    target_path,
                    forward_sql,
                    reverse_sql,
                    objs,
                    created,
                ),
            )
            conn.commit()
        finally:
            conn.close()

        return MigrationInfo(
            id=mig_id,
            name=name,
            sequence=seq,
            source_path=source_path,
            target_path=target_path,
            forward_sql=forward_sql,
            reverse_sql=reverse_sql,
            objects_touched=objs,
            created_at=created,
            applied=False,
            applied_at=None,
        )

    def list(self) -> list[MigrationInfo]:
        """Return all migrations ordered by sequence."""
        conn = self._connect()
        try:
            rows = conn.execute(
                "SELECT id, name, sequence, source_path, target_path, "
                "forward_sql, reverse_sql, objects_touched, created_at, "
                "applied, applied_at "
                "FROM migrations ORDER BY sequence ASC"
            ).fetchall()
        finally:
            conn.close()
        return [self._row_to_info(r) for r in rows]

    def get(self, mig_id: str) -> MigrationInfo | None:
        """Return a single migration by *mig_id*, or ``None``."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT id, name, sequence, source_path, target_path, "
                "forward_sql, reverse_sql, objects_touched, created_at, "
                "applied, applied_at "
                "FROM migrations WHERE id = ?",
                (mig_id,),
            ).fetchone()
        finally:
            conn.close()

        if row is None:
            return None
        return self._row_to_info(row)

    def delete(self, mig_id: str) -> bool:
        """Delete a migration by *mig_id*.

        Returns ``True`` if the migration existed and was deleted.
        """
        info = self.get(mig_id)
        if info is None:
            return False

        conn = self._connect()
        try:
            conn.execute("DELETE FROM migrations WHERE id = ?", (mig_id,))
            conn.commit()
        finally:
            conn.close()
        return True

    def mark_applied(self, mig_id: str) -> bool:
        """Mark a migration as applied.

        Returns ``True`` if the migration was found and updated.
        """
        info = self.get(mig_id)
        if info is None:
            return False

        applied_at = datetime.now(UTC).isoformat()
        conn = self._connect()
        try:
            conn.execute(
                "UPDATE migrations SET applied = 1, applied_at = ? WHERE id = ?",
                (applied_at, mig_id),
            )
            conn.commit()
        finally:
            conn.close()
        return True

    def mark_unapplied(self, mig_id: str) -> bool:
        """Mark a migration as unapplied (rolled back).

        Returns ``True`` if the migration was found and updated.
        """
        info = self.get(mig_id)
        if info is None:
            return False

        conn = self._connect()
        try:
            conn.execute(
                "UPDATE migrations SET applied = 0, applied_at = NULL WHERE id = ?",
                (mig_id,),
            )
            conn.commit()
        finally:
            conn.close()
        return True

    def get_by_sequence_range(self, start: int, end: int) -> list[MigrationInfo]:
        """Return migrations with sequences in [start, end] inclusive."""
        conn = self._connect()
        try:
            rows = conn.execute(
                "SELECT id, name, sequence, source_path, target_path, "
                "forward_sql, reverse_sql, objects_touched, created_at, "
                "applied, applied_at "
                "FROM migrations WHERE sequence >= ? AND sequence <= ? "
                "ORDER BY sequence ASC",
                (start, end),
            ).fetchall()
        finally:
            conn.close()
        return [self._row_to_info(r) for r in rows]

    def replace_with_squashed(
        self,
        old_ids: list[str],
        *,
        source_path: str,
        target_path: str,
        forward_sql: str,
        reverse_sql: str,
        objects_touched: list[str] | None = None,
        name: str | None = None,
    ) -> MigrationInfo:
        """Delete *old_ids* and insert a single squashed migration.

        The squashed migration gets the lowest sequence number from the
        removed set. Remaining migration sequences are left as-is.
        """
        conn = self._connect()
        try:
            # Find the lowest sequence among the old migrations.
            placeholders = ",".join("?" for _ in old_ids)
            row = conn.execute(
                f"SELECT MIN(sequence) FROM migrations WHERE id IN ({placeholders})",
                old_ids,
            ).fetchone()
            min_seq = row[0] if row and row[0] is not None else self._next_sequence()

            # Delete old migrations.
            conn.execute(
                f"DELETE FROM migrations WHERE id IN ({placeholders})",
                old_ids,
            )

            # Insert squashed.
            mig_id = str(uuid.uuid4())
            created = datetime.now(UTC).isoformat()
            objs = ",".join(sorted(objects_touched or []))

            conn.execute(
                "INSERT INTO migrations "
                "(id, name, sequence, source_path, target_path, "
                "forward_sql, reverse_sql, objects_touched, created_at, applied) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
                (
                    mig_id,
                    name,
                    min_seq,
                    source_path,
                    target_path,
                    forward_sql,
                    reverse_sql,
                    objs,
                    created,
                ),
            )
            conn.commit()
        finally:
            conn.close()

        return MigrationInfo(
            id=mig_id,
            name=name,
            sequence=min_seq,
            source_path=source_path,
            target_path=target_path,
            forward_sql=forward_sql,
            reverse_sql=reverse_sql,
            objects_touched=objs,
            created_at=created,
            applied=False,
            applied_at=None,
        )

    # -- internal -----------------------------------------------------------

    def _connect(self) -> sqlite3.Connection:
        return sqlite3.connect(str(self._db_path))

    def _next_sequence(self) -> int:
        conn = self._connect()
        try:
            row = conn.execute("SELECT MAX(sequence) FROM migrations").fetchone()
        finally:
            conn.close()
        if row is None or row[0] is None:
            return 1
        return int(row[0]) + 1

    @staticmethod
    def _row_to_info(row: tuple[object, ...]) -> MigrationInfo:
        return MigrationInfo(
            id=str(row[0]),
            name=str(row[1]) if row[1] is not None else None,
            sequence=int(row[2]),  # type: ignore[arg-type]
            source_path=str(row[3]),
            target_path=str(row[4]),
            forward_sql=str(row[5]),
            reverse_sql=str(row[6]),
            objects_touched=str(row[7]),
            created_at=str(row[8]),
            applied=bool(row[9]),
            applied_at=str(row[10]) if row[10] is not None else None,
        )
