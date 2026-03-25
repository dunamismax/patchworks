"""Comprehensive tests for the SQLite inspection engine.

Covers schema reading, row pagination, streaming iteration,
empty databases, WAL-mode databases, and WITHOUT ROWID tables.
"""

from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from pathlib import Path

import pytest

from patchworks.db.inspector import (
    _open_readonly,
    for_each_row,
    inspect_database,
    inspect_table,
    read_rows,
)
from patchworks.db.types import (
    DatabaseSummary,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _create_db(path: Path, statements: list[str]) -> Path:
    """Create a SQLite database at *path* by executing *statements*."""
    conn = sqlite3.connect(str(path))
    for stmt in statements:
        conn.execute(stmt)
    conn.commit()
    conn.close()
    return path


def _tmp_db(tmp_path: Path, name: str = "test.db") -> Path:
    return tmp_path / name


# ---------------------------------------------------------------------------
# Basic inspection
# ---------------------------------------------------------------------------


class TestInspectDatabase:
    """Tests for ``inspect_database``."""

    def test_empty_database(self, tmp_path: Path) -> None:
        """An empty database returns a valid summary with no tables."""
        db = _create_db(_tmp_db(tmp_path), [])
        summary = inspect_database(db)

        assert isinstance(summary, DatabaseSummary)
        assert summary.tables == ()
        assert summary.views == ()
        assert summary.indexes == ()
        assert summary.triggers == ()
        assert summary.page_size > 0
        assert summary.page_count >= 0
        assert summary.path == str(db.resolve())

    def test_single_table(self, tmp_path: Path) -> None:
        """A database with one table is inspected correctly."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE users ("
                "id INTEGER PRIMARY KEY, "
                "name TEXT NOT NULL, "
                "email TEXT DEFAULT 'none')",
                "INSERT INTO users (name, email) VALUES ('alice', 'alice@example.com')",
                "INSERT INTO users (name, email) VALUES ('bob', 'bob@example.com')",
            ],
        )
        summary = inspect_database(db)

        assert len(summary.tables) == 1
        t = summary.tables[0]
        assert t.name == "users"
        assert t.row_count == 2
        assert len(t.columns) == 3

        id_col = t.columns[0]
        assert id_col.name == "id"
        assert id_col.type == "INTEGER"
        assert id_col.primary_key == 1

        name_col = t.columns[1]
        assert name_col.name == "name"
        assert name_col.notnull is True

        email_col = t.columns[2]
        assert email_col.default_value == "'none'"

    def test_multiple_tables_sorted(self, tmp_path: Path) -> None:
        """Tables are returned sorted by name."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE zebra (id INTEGER PRIMARY KEY)",
                "CREATE TABLE alpha (id INTEGER PRIMARY KEY)",
                "CREATE TABLE middle (id INTEGER PRIMARY KEY)",
            ],
        )
        summary = inspect_database(db)

        names = [t.name for t in summary.tables]
        assert names == ["alpha", "middle", "zebra"]

    def test_primary_key_columns(self, tmp_path: Path) -> None:
        """Composite primary keys are detected in order."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE composite (a TEXT, b INTEGER, c REAL, PRIMARY KEY (b, a))"],
        )
        summary = inspect_database(db)
        t = summary.tables[0]
        assert t.primary_key_columns == ("b", "a")

    def test_table_sql_preserved(self, tmp_path: Path) -> None:
        """The original CREATE TABLE statement is captured."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE things (id INTEGER PRIMARY KEY, val TEXT)"],
        )
        summary = inspect_database(db)
        assert "CREATE TABLE things" in summary.tables[0].sql

    def test_indexes_detected(self, tmp_path: Path) -> None:
        """Explicit indexes are found on their parent table."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, category TEXT)",
                "CREATE INDEX idx_name ON items (name)",
                "CREATE UNIQUE INDEX idx_cat ON items (category)",
            ],
        )
        summary = inspect_database(db)
        t = summary.tables[0]

        idx_names = {i.name for i in t.indexes}
        assert "idx_name" in idx_names
        assert "idx_cat" in idx_names

        cat_idx = next(i for i in t.indexes if i.name == "idx_cat")
        assert cat_idx.unique is True
        assert cat_idx.columns == ("category",)
        assert cat_idx.table_name == "items"

        # Also in the top-level indexes list
        all_idx_names = {i.name for i in summary.indexes}
        assert "idx_name" in all_idx_names
        assert "idx_cat" in all_idx_names

    def test_triggers_detected(self, tmp_path: Path) -> None:
        """Triggers are captured with their SQL."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE log (id INTEGER PRIMARY KEY, msg TEXT)",
                "CREATE TABLE events (id INTEGER PRIMARY KEY, data TEXT)",
                "CREATE TRIGGER after_insert AFTER INSERT "
                "ON events BEGIN "
                "INSERT INTO log (msg) VALUES ('event'); END",
            ],
        )
        summary = inspect_database(db)

        event_table = next(t for t in summary.tables if t.name == "events")
        assert len(event_table.triggers) == 1
        assert event_table.triggers[0].name == "after_insert"
        assert "CREATE TRIGGER" in event_table.triggers[0].sql

        assert len(summary.triggers) == 1

    def test_views_detected(self, tmp_path: Path) -> None:
        """Views are inspected with columns and SQL."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE users ("
                "id INTEGER PRIMARY KEY, name TEXT, active INTEGER)",
                "CREATE VIEW active_users AS "
                "SELECT id, name FROM users WHERE active = 1",
            ],
        )
        summary = inspect_database(db)

        assert len(summary.views) == 1
        v = summary.views[0]
        assert v.name == "active_users"
        assert len(v.columns) == 2
        assert v.columns[0].name == "id"
        assert v.columns[1].name == "name"
        assert "CREATE VIEW" in v.sql

    def test_journal_mode(self, tmp_path: Path) -> None:
        """Journal mode is reported."""
        db = _create_db(_tmp_db(tmp_path), [])
        summary = inspect_database(db)
        # Default is usually "delete" but depends on SQLite build.
        assert summary.journal_mode in (
            "delete",
            "wal",
            "memory",
            "off",
            "truncate",
            "persist",
        )


# ---------------------------------------------------------------------------
# WITHOUT ROWID
# ---------------------------------------------------------------------------


class TestWithoutRowid:
    """Tests for WITHOUT ROWID table handling."""

    def test_without_rowid_detected(self, tmp_path: Path) -> None:
        """WITHOUT ROWID tables are correctly identified."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE kv (key TEXT PRIMARY KEY, val BLOB) WITHOUT ROWID",
                "INSERT INTO kv VALUES ('a', x'01')",
                "INSERT INTO kv VALUES ('b', x'02')",
            ],
        )
        summary = inspect_database(db)
        t = summary.tables[0]
        assert t.without_rowid is True
        assert t.primary_key_columns == ("key",)
        assert t.row_count == 2

    def test_regular_table_not_without_rowid(self, tmp_path: Path) -> None:
        """Regular tables are not marked WITHOUT ROWID."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE normal (id INTEGER PRIMARY KEY)"],
        )
        summary = inspect_database(db)
        assert summary.tables[0].without_rowid is False

    def test_without_rowid_rows_readable(self, tmp_path: Path) -> None:
        """Rows from WITHOUT ROWID tables can be read."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE kv (key TEXT PRIMARY KEY, val TEXT) WITHOUT ROWID",
                "INSERT INTO kv VALUES ('x', '1')",
                "INSERT INTO kv VALUES ('y', '2')",
                "INSERT INTO kv VALUES ('z', '3')",
            ],
        )
        conn = _open_readonly(db)
        try:
            rows = read_rows(
                conn,
                "kv",
                pk_columns=("key",),
                without_rowid=True,
                page_size=10,
            )
            assert len(rows) == 3
            keys = [r["key"] for r in rows]
            assert keys == sorted(keys)
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# WAL mode
# ---------------------------------------------------------------------------


class TestWalMode:
    """Tests for WAL-mode database handling."""

    def test_wal_mode_database(self, tmp_path: Path) -> None:
        """Databases set to WAL mode can be inspected."""
        db = _tmp_db(tmp_path)
        conn = sqlite3.connect(str(db))
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("CREATE TABLE data (id INTEGER PRIMARY KEY, value TEXT)")
        conn.execute("INSERT INTO data VALUES (1, 'hello')")
        conn.execute("INSERT INTO data VALUES (2, 'world')")
        conn.commit()
        conn.close()

        summary = inspect_database(db)
        assert summary.journal_mode == "wal"
        assert len(summary.tables) == 1
        assert summary.tables[0].row_count == 2

    def test_wal_mode_rows(self, tmp_path: Path) -> None:
        """Rows from WAL-mode databases are readable via pagination."""
        db = _tmp_db(tmp_path)
        conn = sqlite3.connect(str(db))
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("CREATE TABLE nums (id INTEGER PRIMARY KEY, n INT)")
        for i in range(50):
            conn.execute("INSERT INTO nums VALUES (?, ?)", (i, i * 10))
        conn.commit()
        conn.close()

        ro_conn = _open_readonly(db)
        try:
            page1 = read_rows(ro_conn, "nums", page_size=20, pk_columns=("id",))
            assert len(page1) == 20
            assert page1[0]["id"] == 0

            page2 = read_rows(
                ro_conn, "nums", page_size=20, offset=20, pk_columns=("id",)
            )
            assert len(page2) == 20
            assert page2[0]["id"] == 20
        finally:
            ro_conn.close()


# ---------------------------------------------------------------------------
# Row pagination
# ---------------------------------------------------------------------------


class TestReadRows:
    """Tests for ``read_rows`` and ``for_each_row``."""

    def test_basic_pagination(self, tmp_path: Path) -> None:
        """Rows are returned in pages of the requested size."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                *[f"INSERT INTO t VALUES ({i}, 'row{i}')" for i in range(25)],
            ],
        )
        conn = _open_readonly(db)
        try:
            p1 = read_rows(conn, "t", page_size=10, pk_columns=("id",))
            assert len(p1) == 10
            assert p1[0]["id"] == 0
            assert p1[9]["id"] == 9

            p2 = read_rows(conn, "t", page_size=10, offset=10, pk_columns=("id",))
            assert len(p2) == 10
            assert p2[0]["id"] == 10

            p3 = read_rows(conn, "t", page_size=10, offset=20, pk_columns=("id",))
            assert len(p3) == 5
        finally:
            conn.close()

    def test_empty_table(self, tmp_path: Path) -> None:
        """An empty table returns no rows."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE empty_t (id INTEGER PRIMARY KEY)"],
        )
        conn = _open_readonly(db)
        try:
            rows = read_rows(conn, "empty_t", pk_columns=("id",))
            assert rows == []
        finally:
            conn.close()

    def test_deterministic_order_with_pk(self, tmp_path: Path) -> None:
        """Rows are returned in PK order when PK columns are specified."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (30, 'c')",
                "INSERT INTO t VALUES (10, 'a')",
                "INSERT INTO t VALUES (20, 'b')",
            ],
        )
        conn = _open_readonly(db)
        try:
            rows = read_rows(conn, "t", pk_columns=("id",))
            ids = [r["id"] for r in rows]
            assert ids == [10, 20, 30]
        finally:
            conn.close()

    def test_deterministic_order_without_pk_uses_rowid(self, tmp_path: Path) -> None:
        """Without PK columns, rowid ordering is used for regular tables."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE t (a TEXT, b TEXT)",
                "INSERT INTO t VALUES ('z', '1')",
                "INSERT INTO t VALUES ('a', '2')",
                "INSERT INTO t VALUES ('m', '3')",
            ],
        )
        conn = _open_readonly(db)
        try:
            rows = read_rows(conn, "t")
            # rowid order matches insertion order.
            assert [r["a"] for r in rows] == ["z", "a", "m"]
        finally:
            conn.close()

    def test_for_each_row_streams_all(self, tmp_path: Path) -> None:
        """``for_each_row`` yields every row across multiple pages."""
        n = 75
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY)",
                *[f"INSERT INTO t VALUES ({i})" for i in range(n)],
            ],
        )
        conn = _open_readonly(db)
        try:
            collected: list[dict[str, Any]] = list(
                for_each_row(conn, "t", page_size=10, pk_columns=("id",))
            )
            assert len(collected) == n
            assert collected[0]["id"] == 0
            assert collected[-1]["id"] == n - 1
        finally:
            conn.close()

    def test_for_each_row_empty_table(self, tmp_path: Path) -> None:
        """``for_each_row`` on an empty table yields nothing."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )
        conn = _open_readonly(db)
        try:
            collected = list(for_each_row(conn, "t", pk_columns=("id",)))
            assert collected == []
        finally:
            conn.close()

    def test_composite_pk_ordering(self, tmp_path: Path) -> None:
        """Rows are ordered by composite PK columns."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE t (a TEXT, b INT, PRIMARY KEY (a, b))",
                "INSERT INTO t VALUES ('b', 2)",
                "INSERT INTO t VALUES ('a', 1)",
                "INSERT INTO t VALUES ('a', 2)",
                "INSERT INTO t VALUES ('b', 1)",
            ],
        )
        conn = _open_readonly(db)
        try:
            rows = read_rows(conn, "t", pk_columns=("a", "b"))
            pairs = [(r["a"], r["b"]) for r in rows]
            assert pairs == [("a", 1), ("a", 2), ("b", 1), ("b", 2)]
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# Read-only enforcement
# ---------------------------------------------------------------------------


class TestReadOnly:
    """Verify databases are opened read-only."""

    def test_cannot_write_through_readonly_connection(self, tmp_path: Path) -> None:
        """Writes through the read-only connection fail."""
        db = _create_db(
            _tmp_db(tmp_path),
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )
        conn = _open_readonly(db)
        try:
            with pytest.raises(sqlite3.OperationalError):
                conn.execute("INSERT INTO t VALUES (1)")
        finally:
            conn.close()


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    """Miscellaneous edge cases."""

    def test_table_with_no_explicit_pk(self, tmp_path: Path) -> None:
        """A table with no explicit PK has empty primary_key_columns."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE loose (a TEXT, b INT)",
                "INSERT INTO loose VALUES ('x', 1)",
            ],
        )
        summary = inspect_database(db)
        t = summary.tables[0]
        assert t.primary_key_columns == ()
        assert t.without_rowid is False
        assert t.row_count == 1

    def test_typeless_columns(self, tmp_path: Path) -> None:
        """Columns with no declared type are handled."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE flex (id, val)",
                "INSERT INTO flex VALUES (1, 'hello')",
            ],
        )
        summary = inspect_database(db)
        t = summary.tables[0]
        assert t.columns[0].type == ""
        assert t.columns[1].type == ""

    def test_special_characters_in_names(self, tmp_path: Path) -> None:
        """Table and column names with special characters are handled."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                'CREATE TABLE "my table" ("col 1" INTEGER PRIMARY KEY, "col-2" TEXT)',
                "INSERT INTO \"my table\" VALUES (1, 'test')",
            ],
        )
        summary = inspect_database(db)
        assert len(summary.tables) == 1
        t = summary.tables[0]
        assert t.name == "my table"
        assert t.columns[0].name == "col 1"
        assert t.columns[1].name == "col-2"
        assert t.row_count == 1

    def test_nonexistent_database_raises(self, tmp_path: Path) -> None:
        """Inspecting a nonexistent file raises an error."""
        with pytest.raises(sqlite3.OperationalError):
            inspect_database(tmp_path / "nope.db")

    def test_multiple_views(self, tmp_path: Path) -> None:
        """Multiple views are all captured."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE base (id INTEGER PRIMARY KEY, x INT, y INT)",
                "CREATE VIEW v1 AS SELECT id, x FROM base",
                "CREATE VIEW v2 AS SELECT id, y FROM base",
            ],
        )
        summary = inspect_database(db)
        view_names = [v.name for v in summary.views]
        assert "v1" in view_names
        assert "v2" in view_names

    def test_partial_index(self, tmp_path: Path) -> None:
        """Partial indexes are detected."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE items (id INTEGER PRIMARY KEY, status TEXT, val INT)",
                "CREATE INDEX idx_active ON items (val) WHERE status = 'active'",
            ],
        )
        summary = inspect_database(db)
        t = summary.tables[0]
        idx = next(i for i in t.indexes if i.name == "idx_active")
        assert idx.partial is True

    def test_inspect_table_directly(self, tmp_path: Path) -> None:
        """``inspect_table`` works when called with an open connection."""
        db = _create_db(
            _tmp_db(tmp_path),
            [
                "CREATE TABLE direct (id INTEGER PRIMARY KEY, val TEXT)",
                "INSERT INTO direct VALUES (1, 'a')",
            ],
        )
        conn = _open_readonly(db)
        try:
            t = inspect_table(conn, "direct")
            assert t.name == "direct"
            assert t.row_count == 1
            assert t.primary_key_columns == ("id",)
        finally:
            conn.close()
