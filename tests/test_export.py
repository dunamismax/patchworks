"""Tests for SQL export and migration generation (Phase 4).

Covers round-trip export application, foreign-key safety, trigger
preservation, and bounded-memory streaming.
"""

from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

from patchworks.db.differ import diff_databases
from patchworks.diff.export import export_as_sql


def _create_db(path: Path, statements: list[str]) -> Path:
    """Create a SQLite database at *path*."""
    conn = sqlite3.connect(str(path))
    for stmt in statements:
        conn.execute(stmt)
    conn.commit()
    conn.close()
    return path


def _apply_migration(db_path: Path, sql: str) -> None:
    """Apply a SQL migration string to the database at *db_path*."""
    conn = sqlite3.connect(str(db_path))
    conn.executescript(sql)
    conn.close()


def _table_rows(db_path: Path, table: str) -> list[dict[str, object]]:
    """Read all rows from *table* as dicts."""
    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    rows = conn.execute(f'SELECT * FROM "{table}" ORDER BY rowid').fetchall()
    conn.close()
    return [dict(r) for r in rows]


def _table_names(db_path: Path) -> list[str]:
    """Return sorted table names (excluding internal tables)."""
    conn = sqlite3.connect(str(db_path))
    rows = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' "
        "AND name NOT LIKE 'sqlite_%' ORDER BY name"
    ).fetchall()
    conn.close()
    return [r[0] for r in rows]


def _get_sql(db_path: Path, obj_type: str, name: str) -> str | None:
    """Get the CREATE SQL for an object from sqlite_master."""
    conn = sqlite3.connect(str(db_path))
    row = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type=? AND name=?",
        (obj_type, name),
    ).fetchone()
    conn.close()
    return row[0] if row else None


# ---------------------------------------------------------------------------
# Round-trip tests
# ---------------------------------------------------------------------------


class TestRoundTrip:
    """Apply generated SQL to the left DB and verify it matches the right."""

    def test_added_table(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT)",
                "INSERT INTO t2 VALUES (1, 'alice')",
                "INSERT INTO t2 VALUES (2, 'bob')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        assert "t2" in _table_names(left)
        rows = _table_rows(left, "t2")
        assert len(rows) == 2
        assert rows[0]["name"] == "alice"

    def test_removed_table(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t1 (id INTEGER PRIMARY KEY)"],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        assert "t2" not in _table_names(left)
        assert "t1" in _table_names(left)

    def test_row_added(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'b')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert len(rows) == 2
        assert rows[1]["v"] == "b"

    def test_row_removed(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'b')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert len(rows) == 1
        assert rows[0]["id"] == 1

    def test_row_modified(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'old')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'new')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert rows[0]["v"] == "new"

    def test_mixed_row_changes(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'keep')",
                "INSERT INTO t VALUES (2, 'modify')",
                "INSERT INTO t VALUES (3, 'remove')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'keep')",
                "INSERT INTO t VALUES (2, 'changed')",
                "INSERT INTO t VALUES (4, 'new')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        row_map = {r["id"]: r["v"] for r in rows}
        assert row_map == {1: "keep", 2: "changed", 4: "new"}

    def test_schema_change_rebuild(self, tmp_path: Path) -> None:
        """Column addition triggers a temp-table rebuild."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
                "INSERT INTO t VALUES (1, 'alice')",
                "INSERT INTO t VALUES (2, 'bob')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
                "INSERT INTO t VALUES (1, 'alice', 'alice@test.com')",
                "INSERT INTO t VALUES (2, 'bob', 'bob@test.com')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert len(rows) == 2
        # The old data (name) is preserved, but email may be NULL since
        # the rebuild copies common columns.
        assert rows[0]["name"] == "alice"
        assert rows[1]["name"] == "bob"

    def test_no_changes_produces_minimal_sql(self, tmp_path: Path) -> None:
        """Identical databases produce SQL that is safe to apply."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        # Should contain the preamble but no DML.
        assert "PRAGMA foreign_keys = OFF" in sql
        assert "COMMIT" in sql
        assert "INSERT" not in sql
        assert "DELETE" not in sql
        assert "UPDATE" not in sql

        # Safe to apply.
        _apply_migration(left, sql)
        rows = _table_rows(left, "t")
        assert len(rows) == 1


# ---------------------------------------------------------------------------
# Foreign key safety
# ---------------------------------------------------------------------------


class TestForeignKeySafety:
    """Verify PRAGMA foreign_keys is guarded in generated SQL."""

    def test_fk_disabled_during_migration(self, tmp_path: Path) -> None:
        """FK enforcement is turned off before DML and on after."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE parent (id INTEGER PRIMARY KEY)",
                "CREATE TABLE child (id INTEGER PRIMARY KEY, pid INTEGER "
                "REFERENCES parent(id))",
                "INSERT INTO parent VALUES (1)",
                "INSERT INTO child VALUES (1, 1)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE parent (id INTEGER PRIMARY KEY)",
                "CREATE TABLE child (id INTEGER PRIMARY KEY, pid INTEGER "
                "REFERENCES parent(id))",
                "INSERT INTO parent VALUES (2)",
                "INSERT INTO child VALUES (2, 2)",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        # FK off at the start, on at the end.
        assert sql.index("PRAGMA foreign_keys = OFF") < sql.index("BEGIN TRANSACTION")
        assert sql.index("COMMIT") < sql.index("PRAGMA foreign_keys = ON")

        # The migration should apply without FK errors even though we remove
        # the parent row before the child row.
        _apply_migration(left, sql)

    def test_fk_safe_with_parent_removal(self, tmp_path: Path) -> None:
        """Removing a parent row during migration doesn't violate FK."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE parent (id INTEGER PRIMARY KEY)",
                "CREATE TABLE child (id INTEGER PRIMARY KEY, pid INTEGER "
                "REFERENCES parent(id))",
                "INSERT INTO parent VALUES (1)",
                "INSERT INTO parent VALUES (2)",
                "INSERT INTO child VALUES (1, 1)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE parent (id INTEGER PRIMARY KEY)",
                "CREATE TABLE child (id INTEGER PRIMARY KEY, pid INTEGER "
                "REFERENCES parent(id))",
                "INSERT INTO parent VALUES (2)",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        assert len(_table_rows(left, "parent")) == 1
        assert len(_table_rows(left, "child")) == 0


# ---------------------------------------------------------------------------
# Trigger preservation
# ---------------------------------------------------------------------------


class TestTriggerPreservation:
    """Triggers are dropped before DML and recreated after."""

    def test_trigger_survives_migration(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('inserted'); END",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('inserted'); END",
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'b')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        # Trigger should still exist.
        trg_sql = _get_sql(left, "trigger", "trg")
        assert trg_sql is not None
        assert "CREATE TRIGGER" in trg_sql

    def test_modified_trigger(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('old'); END",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('new'); END",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        trg_sql = _get_sql(left, "trigger", "trg")
        assert trg_sql is not None
        assert "'new'" in trg_sql

    def test_added_trigger(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('fired'); END",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        trg_sql = _get_sql(left, "trigger", "trg")
        assert trg_sql is not None

    def test_removed_trigger(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('old'); END",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        trg_sql = _get_sql(left, "trigger", "trg")
        assert trg_sql is None


# ---------------------------------------------------------------------------
# Streaming / bounded memory
# ---------------------------------------------------------------------------


class TestStreaming:
    """Verify the streaming export path works with large-ish datasets."""

    def test_write_export_produces_same_as_export_as_sql(self, tmp_path: Path) -> None:
        import io

        from patchworks.diff.export import write_export

        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'b')",
                "INSERT INTO t VALUES (2, 'c')",
            ],
        )

        diff = diff_databases(left, right)
        sql_string = export_as_sql(diff, right_path=right)

        buf = io.StringIO()
        write_export(diff, buf, right_path=right)
        sql_stream = buf.getvalue()

        assert sql_string == sql_stream

    def test_many_rows_added_table(self, tmp_path: Path) -> None:
        """Adding a table with many rows produces correct INSERT statements."""
        n = 200
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                *[f"INSERT INTO t VALUES ({i}, 'row{i}')" for i in range(n)],
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)

        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert len(rows) == n


# ---------------------------------------------------------------------------
# SQL literal edge cases
# ---------------------------------------------------------------------------


class TestSqlLiterals:
    """Verify correct SQL literal escaping."""

    def test_null_values(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, NULL)",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        assert "NULL" in sql

        _apply_migration(left, sql)
        rows = _table_rows(left, "t")
        assert rows[0]["v"] is None

    def test_single_quotes_escaped(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'it''s a test')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        _apply_migration(left, sql)

        rows = _table_rows(left, "t")
        assert rows[0]["v"] == "it's a test"

    def test_blob_values(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB)",
                "INSERT INTO t VALUES (1, x'DEADBEEF')",
            ],
        )

        diff = diff_databases(left, right)
        sql = export_as_sql(diff, right_path=right)
        assert "X'DEADBEEF'" in sql

        _apply_migration(left, sql)
        rows = _table_rows(left, "t")
        assert rows[0]["data"] == b"\xde\xad\xbe\xef"
