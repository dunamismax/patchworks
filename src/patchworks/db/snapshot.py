"""Snapshot persistence and local store behaviour.

Manages a metadata SQLite database at ``~/.patchworks/patchworks.db`` and
stores copied database files under ``~/.patchworks/snapshots/<uuid>.sqlite``.
"""

from __future__ import annotations

import shutil
import sqlite3
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path


@dataclass(frozen=True)
class SnapshotInfo:
    """Metadata for a saved snapshot."""

    id: str
    """UUID identifying the snapshot."""
    source: str
    """Resolved path of the original database at snapshot time."""
    name: str | None
    """Optional human-readable label."""
    created_at: str
    """ISO-8601 timestamp in UTC."""
    file_path: str
    """Path to the copied database file on disk."""
    size_bytes: int
    """Size of the snapshot file in bytes."""


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------

_CREATE_SNAPSHOTS_TABLE = """\
CREATE TABLE IF NOT EXISTS snapshots (
    id          TEXT PRIMARY KEY,
    source      TEXT NOT NULL,
    name        TEXT,
    created_at  TEXT NOT NULL,
    file_path   TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL
)
"""

# ---------------------------------------------------------------------------
# Store
# ---------------------------------------------------------------------------


class SnapshotStore:
    """Manage database snapshots under ``~/.patchworks/``."""

    def __init__(self, base_dir: str | Path | None = None) -> None:
        if base_dir is None:
            self._base = Path.home() / ".patchworks"
        else:
            self._base = Path(base_dir)
        self._snapshots_dir = self._base / "snapshots"
        self._db_path = self._base / "patchworks.db"

        # Ensure directories exist.
        self._base.mkdir(parents=True, exist_ok=True)
        self._snapshots_dir.mkdir(parents=True, exist_ok=True)

        # Ensure the metadata table exists.
        conn = self._connect()
        try:
            conn.execute(_CREATE_SNAPSHOTS_TABLE)
            conn.commit()
        finally:
            conn.close()

    # -- public API ---------------------------------------------------------

    def save(self, db_path: str | Path, *, name: str | None = None) -> SnapshotInfo:
        """Copy *db_path* into the snapshot store and record metadata.

        Returns the :class:`SnapshotInfo` for the newly created snapshot.
        Raises :class:`FileNotFoundError` if *db_path* does not exist.
        """
        source = Path(db_path).resolve()
        if not source.exists():
            msg = f"database not found: {source}"
            raise FileNotFoundError(msg)

        snap_id = str(uuid.uuid4())
        dest = self._snapshots_dir / f"{snap_id}.sqlite"
        shutil.copy2(str(source), str(dest))

        size = dest.stat().st_size
        created = datetime.now(UTC).isoformat()

        conn = self._connect()
        try:
            conn.execute(
                "INSERT INTO snapshots "
                "(id, source, name, created_at, file_path, size_bytes) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (snap_id, str(source), name, created, str(dest), size),
            )
            conn.commit()
        finally:
            conn.close()

        return SnapshotInfo(
            id=snap_id,
            source=str(source),
            name=name,
            created_at=created,
            file_path=str(dest),
            size_bytes=size,
        )

    def list(self, *, source: str | None = None) -> list[SnapshotInfo]:
        """Return all snapshots, optionally filtered by *source* path.

        When *source* is given, it is resolved to an absolute path before
        comparison.
        """
        conn = self._connect()
        try:
            if source is not None:
                resolved = str(Path(source).resolve())
                rows = conn.execute(
                    "SELECT id, source, name, created_at, file_path, size_bytes "
                    "FROM snapshots WHERE source = ? ORDER BY created_at DESC",
                    (resolved,),
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT id, source, name, created_at, file_path, size_bytes "
                    "FROM snapshots ORDER BY created_at DESC"
                ).fetchall()
        finally:
            conn.close()

        return [
            SnapshotInfo(
                id=r[0],
                source=r[1],
                name=r[2],
                created_at=r[3],
                file_path=r[4],
                size_bytes=r[5],
            )
            for r in rows
        ]

    def get(self, snap_id: str) -> SnapshotInfo | None:
        """Return a single snapshot by *snap_id*, or ``None``."""
        conn = self._connect()
        try:
            row = conn.execute(
                "SELECT id, source, name, created_at, file_path, size_bytes "
                "FROM snapshots WHERE id = ?",
                (snap_id,),
            ).fetchone()
        finally:
            conn.close()

        if row is None:
            return None
        return SnapshotInfo(
            id=row[0],
            source=row[1],
            name=row[2],
            created_at=row[3],
            file_path=row[4],
            size_bytes=row[5],
        )

    def delete(self, snap_id: str) -> bool:
        """Delete a snapshot by *snap_id*.

        Removes both the metadata record and the copied database file.
        Returns ``True`` if the snapshot existed and was deleted.
        """
        info = self.get(snap_id)
        if info is None:
            return False

        # Remove the file first (tolerate missing file).
        file_path = Path(info.file_path)
        if file_path.exists():
            file_path.unlink()

        conn = self._connect()
        try:
            conn.execute("DELETE FROM snapshots WHERE id = ?", (snap_id,))
            conn.commit()
        finally:
            conn.close()

        return True

    # -- internal -----------------------------------------------------------

    def _connect(self) -> sqlite3.Connection:
        return sqlite3.connect(str(self._db_path))
