"""Tests for snapshot management (Phase 3)."""

from __future__ import annotations

import sqlite3
from pathlib import Path

import pytest

from patchworks.db.snapshot import SnapshotStore


def _create_db(path: Path, statements: list[str] | None = None) -> Path:
    """Create a SQLite database at *path*."""
    conn = sqlite3.connect(str(path))
    for stmt in statements or []:
        conn.execute(stmt)
    conn.commit()
    conn.close()
    return path


class TestSnapshotSave:
    """Tests for saving snapshots."""

    def test_save_creates_snapshot(self, tmp_path: Path) -> None:
        db = _create_db(
            tmp_path / "test.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )
        store = SnapshotStore(base_dir=tmp_path / "store")

        info = store.save(db)

        assert info.id
        assert info.source == str(db.resolve())
        assert info.name is None
        assert info.size_bytes > 0
        assert Path(info.file_path).exists()

    def test_save_with_name(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        info = store.save(db, name="my-snapshot")

        assert info.name == "my-snapshot"

    def test_save_copies_database_correctly(self, tmp_path: Path) -> None:
        """The snapshot file is an independent copy of the database."""
        db = _create_db(
            tmp_path / "test.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'hello')",
            ],
        )
        store = SnapshotStore(base_dir=tmp_path / "store")
        info = store.save(db)

        # Read from the snapshot copy.
        conn = sqlite3.connect(info.file_path)
        rows = conn.execute("SELECT v FROM t WHERE id = 1").fetchall()
        conn.close()

        assert rows[0][0] == "hello"

    def test_save_nonexistent_database_raises(self, tmp_path: Path) -> None:
        store = SnapshotStore(base_dir=tmp_path / "store")

        with pytest.raises(FileNotFoundError):
            store.save(tmp_path / "nope.db")

    def test_save_multiple_snapshots(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        s1 = store.save(db, name="first")
        s2 = store.save(db, name="second")

        assert s1.id != s2.id
        assert Path(s1.file_path).exists()
        assert Path(s2.file_path).exists()


class TestSnapshotList:
    """Tests for listing snapshots."""

    def test_list_empty(self, tmp_path: Path) -> None:
        store = SnapshotStore(base_dir=tmp_path / "store")

        result = store.list()

        assert result == []

    def test_list_all(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        store.save(db, name="a")
        store.save(db, name="b")

        result = store.list()

        assert len(result) == 2
        names = {s.name for s in result}
        assert names == {"a", "b"}

    def test_list_with_source_filter(self, tmp_path: Path) -> None:
        db1 = _create_db(tmp_path / "one.db")
        db2 = _create_db(tmp_path / "two.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        store.save(db1, name="from-one")
        store.save(db2, name="from-two")

        result = store.list(source=str(db1))

        assert len(result) == 1
        assert result[0].name == "from-one"

    def test_list_source_filter_no_match(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        store.save(db)

        result = store.list(source=str(tmp_path / "nope.db"))

        assert result == []

    def test_list_ordered_by_created_at_descending(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")

        s1 = store.save(db, name="first")
        s2 = store.save(db, name="second")

        result = store.list()

        # Most recent first.
        assert result[0].id == s2.id
        assert result[1].id == s1.id


class TestSnapshotDelete:
    """Tests for deleting snapshots."""

    def test_delete_existing(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")
        info = store.save(db)

        assert Path(info.file_path).exists()
        result = store.delete(info.id)

        assert result is True
        assert not Path(info.file_path).exists()
        assert store.get(info.id) is None

    def test_delete_nonexistent(self, tmp_path: Path) -> None:
        store = SnapshotStore(base_dir=tmp_path / "store")

        result = store.delete("nonexistent-uuid")

        assert result is False

    def test_delete_removes_metadata(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")
        info = store.save(db)

        store.delete(info.id)

        remaining = store.list()
        assert len(remaining) == 0

    def test_delete_tolerates_missing_file(self, tmp_path: Path) -> None:
        """Delete succeeds even if the snapshot file was already removed."""
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")
        info = store.save(db)

        # Manually remove the file.
        Path(info.file_path).unlink()

        result = store.delete(info.id)
        assert result is True
        assert store.get(info.id) is None


class TestSnapshotGet:
    """Tests for retrieving a single snapshot."""

    def test_get_existing(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        store = SnapshotStore(base_dir=tmp_path / "store")
        info = store.save(db, name="test")

        result = store.get(info.id)

        assert result is not None
        assert result.id == info.id
        assert result.name == "test"

    def test_get_nonexistent(self, tmp_path: Path) -> None:
        store = SnapshotStore(base_dir=tmp_path / "store")

        result = store.get("nonexistent")

        assert result is None


class TestSnapshotEdgeCases:
    """Edge cases for the snapshot store."""

    def test_store_creates_directories(self, tmp_path: Path) -> None:
        """Store auto-creates base and snapshots directories."""
        base = tmp_path / "deep" / "nested" / "store"
        SnapshotStore(base_dir=base)

        assert base.exists()
        assert (base / "snapshots").exists()
        assert (base / "patchworks.db").exists()

    def test_multiple_stores_same_dir(self, tmp_path: Path) -> None:
        """Multiple SnapshotStore instances on the same dir are consistent."""
        base = tmp_path / "store"
        db = _create_db(tmp_path / "test.db")

        s1 = SnapshotStore(base_dir=base)
        info = s1.save(db, name="from-s1")

        s2 = SnapshotStore(base_dir=base)
        result = s2.list()

        assert len(result) == 1
        assert result[0].id == info.id
