"""Comprehensive tests for schema and row-level diffing.

Covers schema diffs, row diffs, mixed changes, edge cases,
streaming behaviour, and the high-level orchestrator.
"""

from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

from patchworks.db.differ import diff_databases
from patchworks.db.inspector import _open_readonly, inspect_database, inspect_table
from patchworks.diff.data import diff_table_data
from patchworks.diff.schema import diff_schemas

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


# ---------------------------------------------------------------------------
# Schema diffing
# ---------------------------------------------------------------------------


class TestSchemaDiffTables:
    """Schema diffs for tables: added, removed, modified."""

    def test_identical_databases(self, tmp_path: Path) -> None:
        """Two identical databases produce no schema diff."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        assert not sd.has_changes

    def test_table_added(self, tmp_path: Path) -> None:
        """A table present in right but not left is detected as added."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.tables_added) == 1
        assert sd.tables_added[0].name == "t2"
        assert len(sd.tables_removed) == 0

    def test_table_removed(self, tmp_path: Path) -> None:
        """A table present in left but not right is detected as removed."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.tables_removed) == 1
        assert sd.tables_removed[0].name == "t2"
        assert len(sd.tables_added) == 0

    def test_table_modified_column_added(self, tmp_path: Path) -> None:
        """A table with a new column is detected as modified."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.tables_modified) == 1
        mod = sd.tables_modified[0]
        assert mod.table_name == "t"
        assert len(mod.columns_added) == 1
        assert mod.columns_added[0].name == "email"
        assert len(mod.columns_removed) == 0

    def test_table_modified_column_removed(self, tmp_path: Path) -> None:
        """A table with a removed column is detected as modified."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.tables_modified) == 1
        mod = sd.tables_modified[0]
        assert len(mod.columns_removed) == 1
        assert mod.columns_removed[0].name == "email"

    def test_table_modified_column_type_changed(self, tmp_path: Path) -> None:
        """A column type change is detected."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, val INTEGER)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.tables_modified) == 1
        mod = sd.tables_modified[0]
        assert len(mod.columns_modified) == 1
        old_col, new_col = mod.columns_modified[0]
        assert old_col.type == "TEXT"
        assert new_col.type == "INTEGER"

    def test_multiple_changes(self, tmp_path: Path) -> None:
        """Multiple simultaneous table changes are all detected."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
                "CREATE TABLE drop_me (id INTEGER PRIMARY KEY)",
                "CREATE TABLE modify_me (id INTEGER PRIMARY KEY, val TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
                "CREATE TABLE new_one (id INTEGER PRIMARY KEY)",
                "CREATE TABLE modify_me (id INTEGER PRIMARY KEY, val INTEGER)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert {t.name for t in sd.tables_added} == {"new_one"}
        assert {t.name for t in sd.tables_removed} == {"drop_me"}
        assert len(sd.tables_modified) == 1
        assert sd.tables_modified[0].table_name == "modify_me"


class TestSchemaDiffIndexes:
    """Schema diffs for indexes."""

    def test_index_added(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
                "CREATE INDEX idx_name ON t (name)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.indexes_added) == 1
        assert sd.indexes_added[0].name == "idx_name"

    def test_index_removed(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
                "CREATE INDEX idx_name ON t (name)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.indexes_removed) == 1
        assert sd.indexes_removed[0].name == "idx_name"

    def test_index_modified(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, val INT)",
                "CREATE INDEX idx_name ON t (name)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, val INT)",
                "CREATE UNIQUE INDEX idx_name ON t (name, val)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.indexes_modified) == 1
        assert sd.indexes_modified[0].name == "idx_name"
        assert sd.indexes_modified[0].old_sql != sd.indexes_modified[0].new_sql


class TestSchemaDiffTriggers:
    """Schema diffs for triggers."""

    def test_trigger_added(self, tmp_path: Path) -> None:
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
                "BEGIN INSERT INTO log VALUES ('inserted'); END",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.triggers_added) == 1
        assert sd.triggers_added[0].name == "trg"

    def test_trigger_removed(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
                "CREATE TRIGGER trg AFTER INSERT ON t "
                "BEGIN INSERT INTO log VALUES ('inserted'); END",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE log (msg TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.triggers_removed) == 1
        assert sd.triggers_removed[0].name == "trg"

    def test_trigger_modified(self, tmp_path: Path) -> None:
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

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.triggers_modified) == 1
        assert sd.triggers_modified[0].name == "trg"


class TestSchemaDiffViews:
    """Schema diffs for views."""

    def test_view_added(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE VIEW v1 AS SELECT id FROM t",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.views_added) == 1
        assert sd.views_added[0].name == "v1"

    def test_view_removed(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE VIEW v1 AS SELECT id FROM t",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.views_removed) == 1
        assert sd.views_removed[0].name == "v1"

    def test_view_modified(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, x INT)",
                "CREATE VIEW v1 AS SELECT id FROM t",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, x INT)",
                "CREATE VIEW v1 AS SELECT id, v FROM t",
            ],
        )

        sd = diff_schemas(inspect_database(left), inspect_database(right))

        assert len(sd.views_modified) == 1
        assert sd.views_modified[0].name == "v1"


# ---------------------------------------------------------------------------
# Row-level diffing
# ---------------------------------------------------------------------------


class TestRowDiffBasic:
    """Row-level diffs: added, removed, modified rows."""

    def test_identical_tables(self, tmp_path: Path) -> None:
        """Identical tables produce no row diffs."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 0
        assert td.rows_removed == 0
        assert td.rows_modified == 0
        assert td.row_diffs == ()

    def test_rows_added(self, tmp_path: Path) -> None:
        """New rows in right are detected as added."""
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
                "INSERT INTO t VALUES (3, 'c')",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 2
        assert td.rows_removed == 0
        assert td.rows_modified == 0
        added = [d for d in td.row_diffs if d.kind == "added"]
        assert len(added) == 2
        assert added[0].new_values is not None
        assert added[0].old_values is None

    def test_rows_removed(self, tmp_path: Path) -> None:
        """Rows absent from right are detected as removed."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'b')",
                "INSERT INTO t VALUES (3, 'c')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (2, 'b')",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_removed == 2
        assert td.rows_added == 0
        removed = [d for d in td.row_diffs if d.kind == "removed"]
        assert len(removed) == 2
        assert removed[0].old_values is not None
        assert removed[0].new_values is None

    def test_rows_modified(self, tmp_path: Path) -> None:
        """Changed values are detected with per-cell detail."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score INT)",
                "INSERT INTO t VALUES (1, 'alice', 100)",
                "INSERT INTO t VALUES (2, 'bob', 200)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score INT)",
                "INSERT INTO t VALUES (1, 'alice', 150)",
                "INSERT INTO t VALUES (2, 'bob', 200)",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_modified == 1
        mod = td.row_diffs[0]
        assert mod.kind == "modified"
        assert mod.key == (1,)
        assert len(mod.cell_changes) == 1
        assert mod.cell_changes[0].column == "score"
        assert mod.cell_changes[0].old_value == 100
        assert mod.cell_changes[0].new_value == 150

    def test_mixed_changes(self, tmp_path: Path) -> None:
        """Added, removed, and modified rows in the same table."""
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

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 1
        assert td.rows_removed == 1
        assert td.rows_modified == 1

        kinds = {d.kind for d in td.row_diffs}
        assert kinds == {"added", "removed", "modified"}


class TestRowDiffCompositeKey:
    """Row diffs with composite primary keys."""

    def test_composite_pk_diff(self, tmp_path: Path) -> None:
        """Diffing works with composite primary keys."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (a TEXT, b INT, val TEXT, PRIMARY KEY (a, b))",
                "INSERT INTO t VALUES ('x', 1, 'old')",
                "INSERT INTO t VALUES ('x', 2, 'keep')",
                "INSERT INTO t VALUES ('y', 1, 'remove')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (a TEXT, b INT, val TEXT, PRIMARY KEY (a, b))",
                "INSERT INTO t VALUES ('x', 1, 'new')",
                "INSERT INTO t VALUES ('x', 2, 'keep')",
                "INSERT INTO t VALUES ('z', 1, 'added')",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 1
        assert td.rows_removed == 1
        assert td.rows_modified == 1

        mod = next(d for d in td.row_diffs if d.kind == "modified")
        assert mod.key == ("x", 1)
        assert mod.cell_changes[0].column == "val"


class TestRowDiffRowidFallback:
    """Row diffs when no primary key is defined."""

    def test_no_pk_uses_rowid(self, tmp_path: Path) -> None:
        """Tables without explicit PKs fall back to rowid-based comparison."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (a TEXT, b INT)",
                "INSERT INTO t VALUES ('x', 1)",
                "INSERT INTO t VALUES ('y', 2)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (a TEXT, b INT)",
                "INSERT INTO t VALUES ('x', 1)",
                "INSERT INTO t VALUES ('y', 2)",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 0
        assert td.rows_removed == 0
        assert td.rows_modified == 0
        assert len(td.warnings) > 0
        assert "rowid" in td.warnings[0].lower()

    def test_divergent_pk_warns(self, tmp_path: Path) -> None:
        """Divergent PKs emit a warning and fall back to rowid."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )
        # Right has a different PK structure.
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER, v TEXT PRIMARY KEY)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert len(td.warnings) > 0
        assert "rowid" in td.warnings[0].lower() or "differ" in td.warnings[0].lower()


class TestRowDiffEmpty:
    """Row diffs with empty tables."""

    def test_both_empty(self, tmp_path: Path) -> None:
        """Two empty tables produce no diffs."""
        stmts = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 0
        assert td.rows_removed == 0
        assert td.rows_modified == 0

    def test_left_empty_right_has_rows(self, tmp_path: Path) -> None:
        """All rows in right show as added."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
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

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 2
        assert td.rows_removed == 0

    def test_right_empty_left_has_rows(self, tmp_path: Path) -> None:
        """All rows in left show as removed."""
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
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_removed == 2
        assert td.rows_added == 0


class TestRowDiffStreaming:
    """Verify streaming behaviour with small page sizes."""

    def test_small_page_size(self, tmp_path: Path) -> None:
        """Row diffs are correct with a page size of 1."""
        n = 20
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v INT)",
                *[f"INSERT INTO t VALUES ({i}, {i * 10})" for i in range(n)],
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v INT)",
                # Remove odds, modify evens, add new high ids.
                *[f"INSERT INTO t VALUES ({i}, {i * 100})" for i in range(0, n, 2)],
                *[f"INSERT INTO t VALUES ({n + i}, {i})" for i in range(5)],
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt, page_size=1)
        finally:
            lc.close()
            rc.close()

        # 10 odd IDs removed.
        assert td.rows_removed == 10
        # 5 new IDs added.
        assert td.rows_added == 5
        # 9 even IDs modified (id=0 has v=0 in both; others change from i*10 to i*100).
        assert td.rows_modified == 9

    def test_large_dataset_streaming(self, tmp_path: Path) -> None:
        """Larger dataset works correctly with bounded page size."""
        n = 500
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                *[f"INSERT INTO t VALUES ({i}, 'v{i}')" for i in range(n)],
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                *[f"INSERT INTO t VALUES ({i}, 'v{i}')" for i in range(n)],
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt, page_size=50)
        finally:
            lc.close()
            rc.close()

        assert td.rows_added == 0
        assert td.rows_removed == 0
        assert td.rows_modified == 0


# ---------------------------------------------------------------------------
# High-level orchestrator (diff_databases)
# ---------------------------------------------------------------------------


class TestDiffDatabases:
    """Tests for the top-level ``diff_databases`` function."""

    def test_identical_databases(self, tmp_path: Path) -> None:
        """Identical databases produce no changes."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = diff_databases(left, right)

        assert not result.has_changes
        assert not result.schema_diff.has_changes

    def test_schema_only_mode(self, tmp_path: Path) -> None:
        """``data=False`` skips row-level diffing."""
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
            ],
        )

        result = diff_databases(left, right, data=False)

        # Schema is the same.
        assert not result.schema_diff.has_changes
        # No data diffs computed.
        assert result.table_data_diffs == ()
        # So has_changes is False (schema-only mode doesn't see row changes).
        assert not result.has_changes

    def test_schema_and_row_changes(self, tmp_path: Path) -> None:
        """Both schema and row changes are detected together."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t1 VALUES (1, 'a')",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t1 VALUES (1, 'b')",
                "INSERT INTO t1 VALUES (2, 'c')",
                "CREATE TABLE t3 (id INTEGER PRIMARY KEY)",
            ],
        )

        result = diff_databases(left, right)

        assert result.has_changes
        # t2 removed, t3 added.
        assert len(result.schema_diff.tables_added) == 1
        assert len(result.schema_diff.tables_removed) == 1
        # t1 has row changes.
        t1_diff = next(d for d in result.table_data_diffs if d.table_name == "t1")
        assert t1_diff.rows_added == 1
        assert t1_diff.rows_modified == 1

    def test_column_layout_change_skips_row_diff(self, tmp_path: Path) -> None:
        """When columns change, row diffing is skipped with a warning."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
                "INSERT INTO t VALUES (1, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, extra INT)",
                "INSERT INTO t VALUES (1, 'a', 42)",
            ],
        )

        result = diff_databases(left, right)

        assert result.has_changes
        assert len(result.schema_diff.tables_modified) == 1
        # Row diff should be skipped for this table.
        t_diffs = [d for d in result.table_data_diffs if d.table_name == "t"]
        assert len(t_diffs) == 0
        assert any("column layout changed" in w for w in result.warnings)

    def test_multiple_tables_row_diffs(self, tmp_path: Path) -> None:
        """Row diffs are computed for each common table independently."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'x')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'y')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'x')",
                "INSERT INTO a VALUES (2, 'new')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'changed')",
            ],
        )

        result = diff_databases(left, right)

        a_diff = next(d for d in result.table_data_diffs if d.table_name == "a")
        b_diff = next(d for d in result.table_data_diffs if d.table_name == "b")
        assert a_diff.rows_added == 1
        assert b_diff.rows_modified == 1

    def test_paths_in_result(self, tmp_path: Path) -> None:
        """Result includes resolved paths."""
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = diff_databases(left, right)

        assert str(left.resolve()) in result.left_path
        assert str(right.resolve()) in result.right_path

    def test_without_rowid_tables(self, tmp_path: Path) -> None:
        """WITHOUT ROWID tables are diffed correctly."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE kv (key TEXT PRIMARY KEY, val TEXT) WITHOUT ROWID",
                "INSERT INTO kv VALUES ('a', '1')",
                "INSERT INTO kv VALUES ('b', '2')",
                "INSERT INTO kv VALUES ('c', '3')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE kv (key TEXT PRIMARY KEY, val TEXT) WITHOUT ROWID",
                "INSERT INTO kv VALUES ('a', '1')",
                "INSERT INTO kv VALUES ('b', 'changed')",
                "INSERT INTO kv VALUES ('d', '4')",
            ],
        )

        result = diff_databases(left, right)

        kv_diff = next(d for d in result.table_data_diffs if d.table_name == "kv")
        assert kv_diff.rows_added == 1  # 'd'
        assert kv_diff.rows_removed == 1  # 'c'
        assert kv_diff.rows_modified == 1  # 'b' value changed


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    """Various edge cases for the diff engine."""

    def test_empty_databases(self, tmp_path: Path) -> None:
        """Two empty databases produce no diff."""
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = diff_databases(left, right)

        assert not result.has_changes

    def test_null_values(self, tmp_path: Path) -> None:
        """NULL values are handled correctly in comparisons."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, NULL)",
                "INSERT INTO t VALUES (2, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'now_set')",
                "INSERT INTO t VALUES (2, NULL)",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_modified == 2
        changes_by_id = {d.key[0]: d for d in td.row_diffs}
        # id=1: NULL → 'now_set'
        assert changes_by_id[1].cell_changes[0].old_value is None
        assert changes_by_id[1].cell_changes[0].new_value == "now_set"
        # id=2: 'a' → NULL
        assert changes_by_id[2].cell_changes[0].old_value == "a"
        assert changes_by_id[2].cell_changes[0].new_value is None

    def test_special_characters_in_table_names(self, tmp_path: Path) -> None:
        """Tables with special characters in names are diffed correctly."""
        left = _create_db(
            tmp_path / "left.db",
            [
                'CREATE TABLE "my table" ("col 1" INTEGER PRIMARY KEY, "val-2" TEXT)',
                "INSERT INTO \"my table\" VALUES (1, 'a')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                'CREATE TABLE "my table" ("col 1" INTEGER PRIMARY KEY, "val-2" TEXT)',
                "INSERT INTO \"my table\" VALUES (1, 'b')",
            ],
        )

        result = diff_databases(left, right)

        assert result.has_changes
        td = result.table_data_diffs[0]
        assert td.table_name == "my table"
        assert td.rows_modified == 1

    def test_blob_values(self, tmp_path: Path) -> None:
        """BLOB values are compared correctly."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB)",
                "INSERT INTO t VALUES (1, x'DEADBEEF')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB)",
                "INSERT INTO t VALUES (1, x'CAFEBABE')",
            ],
        )

        lc = _open_readonly(left)
        rc = _open_readonly(right)
        try:
            lt = inspect_table(lc, "t")
            rt = inspect_table(rc, "t")
            td = diff_table_data(lc, rc, lt, rt)
        finally:
            lc.close()
            rc.close()

        assert td.rows_modified == 1
        cc = td.row_diffs[0].cell_changes[0]
        assert cc.column == "data"

    def test_schema_diff_has_changes_property(self, tmp_path: Path) -> None:
        """SchemaDiff.has_changes is False when identical."""
        stmts = ["CREATE TABLE t (id INTEGER PRIMARY KEY)"]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        sd = diff_schemas(inspect_database(left), inspect_database(right))
        assert not sd.has_changes

    def test_database_diff_has_changes_data_only(self, tmp_path: Path) -> None:
        """DatabaseDiff.has_changes is True when only data differs."""
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
            ],
        )

        result = diff_databases(left, right)

        assert not result.schema_diff.has_changes
        assert result.has_changes

    def test_many_tables_diffed(self, tmp_path: Path) -> None:
        """Multiple tables are all diffed independently."""
        n_tables = 10
        left_stmts = [
            f"CREATE TABLE t{i} (id INTEGER PRIMARY KEY, v INT)"
            for i in range(n_tables)
        ]
        left_stmts += [
            f"INSERT INTO t{i} VALUES ({j}, {j})"
            for i in range(n_tables)
            for j in range(5)
        ]
        right_stmts = [
            f"CREATE TABLE t{i} (id INTEGER PRIMARY KEY, v INT)"
            for i in range(n_tables)
        ]
        right_stmts += [
            f"INSERT INTO t{i} VALUES ({j}, {j * 2})"
            for i in range(n_tables)
            for j in range(5)
        ]

        left = _create_db(tmp_path / "left.db", left_stmts)
        right = _create_db(tmp_path / "right.db", right_stmts)

        result = diff_databases(left, right)

        assert len(result.table_data_diffs) == n_tables
        for td in result.table_data_diffs:
            # id=0 has v=0 in both (0*2 == 0), rest differ.
            assert td.rows_modified == 4
