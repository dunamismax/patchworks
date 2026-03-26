"""Comprehensive tests for Phase 6 — advanced diff intelligence.

Covers:
- Table rename detection via column similarity
- Column rename detection via property matching
- Type shift detection and SQLite affinity classification
- Confidence scores
- Diff filtering by change type and table
- Aggregate diff summary statistics
- Data-type-aware comparison
- Diff annotations
- Full semantic analysis pipeline
- Edge cases
"""

from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

    from patchworks.db.types import DiffAnnotation

from patchworks.db.differ import diff_databases
from patchworks.db.inspector import inspect_database
from patchworks.diff.schema import diff_schemas
from patchworks.diff.semantic import (
    analyze,
    annotate,
    detect_column_renames,
    detect_table_renames,
    detect_type_shifts,
    filter_diff,
    is_affinity_compatible,
    semantic_cell_changes,
    sqlite_type_affinity,
    summarize_diff,
    values_semantically_equal,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _create_db(path: Path, statements: list[str]) -> Path:
    conn = sqlite3.connect(str(path))
    for stmt in statements:
        conn.execute(stmt)
    conn.commit()
    conn.close()
    return path


# ---------------------------------------------------------------------------
# SQLite type affinity
# ---------------------------------------------------------------------------


class TestSqliteTypeAffinity:
    """Test SQLite type affinity classification."""

    def test_integer_affinity(self) -> None:
        assert sqlite_type_affinity("INTEGER") == "INTEGER"
        assert sqlite_type_affinity("INT") == "INTEGER"
        assert sqlite_type_affinity("TINYINT") == "INTEGER"
        assert sqlite_type_affinity("SMALLINT") == "INTEGER"
        assert sqlite_type_affinity("MEDIUMINT") == "INTEGER"
        assert sqlite_type_affinity("BIGINT") == "INTEGER"
        assert sqlite_type_affinity("INT8") == "INTEGER"

    def test_text_affinity(self) -> None:
        assert sqlite_type_affinity("TEXT") == "TEXT"
        assert sqlite_type_affinity("VARCHAR(255)") == "TEXT"
        assert sqlite_type_affinity("CHARACTER(100)") == "TEXT"
        assert sqlite_type_affinity("CLOB") == "TEXT"
        assert sqlite_type_affinity("NCHAR(55)") == "TEXT"

    def test_blob_affinity(self) -> None:
        assert sqlite_type_affinity("BLOB") == "BLOB"
        assert sqlite_type_affinity("") == "BLOB"

    def test_real_affinity(self) -> None:
        assert sqlite_type_affinity("REAL") == "REAL"
        assert sqlite_type_affinity("DOUBLE") == "REAL"
        assert sqlite_type_affinity("DOUBLE PRECISION") == "REAL"
        assert sqlite_type_affinity("FLOAT") == "REAL"

    def test_numeric_affinity(self) -> None:
        assert sqlite_type_affinity("NUMERIC") == "NUMERIC"
        assert sqlite_type_affinity("DECIMAL(10,5)") == "NUMERIC"
        assert sqlite_type_affinity("BOOLEAN") == "NUMERIC"
        assert sqlite_type_affinity("DATE") == "NUMERIC"
        assert sqlite_type_affinity("DATETIME") == "NUMERIC"

    def test_case_insensitive(self) -> None:
        assert sqlite_type_affinity("integer") == "INTEGER"
        assert sqlite_type_affinity("Text") == "TEXT"
        assert sqlite_type_affinity("rEaL") == "REAL"


class TestAffinityCompatibility:
    """Test affinity compatibility matrix."""

    def test_same_affinity_compatible(self) -> None:
        for aff in ("INTEGER", "TEXT", "BLOB", "REAL", "NUMERIC"):
            assert is_affinity_compatible(aff, aff) is True

    def test_int_to_real_compatible(self) -> None:
        assert is_affinity_compatible("INTEGER", "REAL") is True

    def test_int_to_numeric_compatible(self) -> None:
        assert is_affinity_compatible("INTEGER", "NUMERIC") is True

    def test_real_to_numeric_compatible(self) -> None:
        assert is_affinity_compatible("REAL", "NUMERIC") is True

    def test_int_to_text_incompatible(self) -> None:
        assert is_affinity_compatible("INTEGER", "TEXT") is False

    def test_int_to_blob_incompatible(self) -> None:
        assert is_affinity_compatible("INTEGER", "BLOB") is False

    def test_real_to_int_incompatible(self) -> None:
        # Lossy: real can't always round-trip to int.
        assert is_affinity_compatible("REAL", "INTEGER") is False

    def test_text_to_blob_incompatible(self) -> None:
        assert is_affinity_compatible("TEXT", "BLOB") is False


# ---------------------------------------------------------------------------
# Data-type-aware comparison
# ---------------------------------------------------------------------------


class TestValuesSemanticEquality:
    """Test data-type-aware value comparison."""

    def test_identical_values(self) -> None:
        assert values_semantically_equal(42, 42) is True
        assert values_semantically_equal("hello", "hello") is True
        assert values_semantically_equal(None, None) is True

    def test_int_vs_float(self) -> None:
        assert values_semantically_equal(1, 1.0) is True
        assert values_semantically_equal(42, 42.0) is True
        assert values_semantically_equal(0, 0.0) is True

    def test_int_vs_float_not_equal(self) -> None:
        assert values_semantically_equal(1, 1.5) is False

    def test_str_vs_int(self) -> None:
        assert values_semantically_equal("42", 42) is True
        assert values_semantically_equal(42, "42") is True

    def test_str_vs_float(self) -> None:
        assert values_semantically_equal("42.0", 42.0) is True
        assert values_semantically_equal(42.0, "42.0") is True
        assert values_semantically_equal("42", 42.0) is True

    def test_str_vs_numeric_not_equal(self) -> None:
        assert values_semantically_equal("hello", 42) is False
        assert values_semantically_equal(42, "hello") is False

    def test_none_vs_value(self) -> None:
        assert values_semantically_equal(None, 42) is False
        assert values_semantically_equal(42, None) is False
        assert values_semantically_equal(None, "") is False

    def test_different_strings(self) -> None:
        assert values_semantically_equal("foo", "bar") is False

    def test_different_types(self) -> None:
        assert values_semantically_equal(b"data", "data") is False


class TestSemanticCellChanges:
    """Test data-type-aware cell change detection."""

    def test_cosmetic_int_float_ignored(self) -> None:
        old = {"id": 1, "val": 42}
        new = {"id": 1, "val": 42.0}
        changes = semantic_cell_changes(old, new)
        assert len(changes) == 0

    def test_cosmetic_str_int_ignored(self) -> None:
        old = {"id": 1, "val": "42"}
        new = {"id": 1, "val": 42}
        changes = semantic_cell_changes(old, new)
        assert len(changes) == 0

    def test_real_change_detected(self) -> None:
        old = {"id": 1, "val": 42}
        new = {"id": 1, "val": 99}
        changes = semantic_cell_changes(old, new)
        assert len(changes) == 1
        assert changes[0].column == "val"

    def test_mixed_cosmetic_and_real(self) -> None:
        old = {"id": 1, "score": 100, "name": "alice"}
        new = {"id": 1, "score": 100.0, "name": "bob"}
        changes = semantic_cell_changes(old, new)
        assert len(changes) == 1
        assert changes[0].column == "name"


# ---------------------------------------------------------------------------
# Table rename detection
# ---------------------------------------------------------------------------


class TestTableRenameDetection:
    """Test table rename detection via column similarity."""

    def test_identical_columns_high_confidence(self, tmp_path: Path) -> None:
        """A table renamed with identical columns should score 1.0."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_table_renames(sd)
        assert len(renames) == 1
        assert renames[0].old_name == "users"
        assert renames[0].new_name == "people"
        assert renames[0].confidence == 1.0
        assert set(renames[0].matched_columns) == {"id", "name", "email"}

    def test_partial_overlap(self, tmp_path: Path) -> None:
        """Partial column overlap yields proportional confidence."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE items "
                "(id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INT)"
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE products "
                "(id INTEGER PRIMARY KEY, name TEXT, cost REAL, stock INT)"
            ],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_table_renames(sd)
        # id, name are shared → 2/6 = 0.333 → below default 0.6 threshold.
        assert len(renames) == 0

    def test_partial_overlap_lower_threshold(self, tmp_path: Path) -> None:
        """Lower threshold catches more aggressive renames."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE items "
                "(id INTEGER PRIMARY KEY, name TEXT, price REAL, qty INT)"
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE products "
                "(id INTEGER PRIMARY KEY, name TEXT, cost REAL, stock INT)"
            ],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_table_renames(sd, threshold=0.3)
        assert len(renames) == 1
        assert renames[0].old_name == "items"
        assert renames[0].new_name == "products"
        assert 0.3 <= renames[0].confidence <= 0.4

    def test_no_removed_tables(self, tmp_path: Path) -> None:
        """No renames when there are no removed tables."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)
        renames = detect_table_renames(sd)
        assert renames == ()

    def test_no_added_tables(self, tmp_path: Path) -> None:
        """No renames when there are no added tables."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY)",
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)
        renames = detect_table_renames(sd)
        assert renames == ()

    def test_greedy_best_match(self, tmp_path: Path) -> None:
        """Multiple candidates are matched greedily (best first)."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, x TEXT, y INT)",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, p TEXT, q INT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE a_renamed (id INTEGER PRIMARY KEY, x TEXT, y INT)",
                "CREATE TABLE b_renamed (id INTEGER PRIMARY KEY, p TEXT, q INT)",
            ],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_table_renames(sd)
        assert len(renames) == 2
        rename_map = {r.old_name: r.new_name for r in renames}
        assert rename_map["a"] == "a_renamed"
        assert rename_map["b"] == "b_renamed"

    def test_completely_different_columns(self, tmp_path: Path) -> None:
        """No renames when column sets are entirely different."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE old_t (a TEXT, b INT, c REAL)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE new_t (x TEXT, y INT, z REAL)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)
        renames = detect_table_renames(sd)
        assert renames == ()


# ---------------------------------------------------------------------------
# Column rename detection
# ---------------------------------------------------------------------------


class TestColumnRenameDetection:
    """Test column rename detection via property matching."""

    def test_column_rename_same_properties(self, tmp_path: Path) -> None:
        """A column renamed with identical properties is detected."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, username TEXT NOT NULL)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, display_name TEXT NOT NULL)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_column_renames(sd)
        assert len(renames) == 1
        assert renames[0].table_name == "t"
        assert renames[0].old_name == "username"
        assert renames[0].new_name == "display_name"
        assert renames[0].confidence >= 0.7

    def test_column_rename_different_type(self, tmp_path: Path) -> None:
        """A rename where the type also changed has lower confidence."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, value INTEGER)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        # With default threshold of 0.7, the rename might not be detected
        # because only notnull, pk, and default match (0.75).
        renames = detect_column_renames(sd)
        assert len(renames) == 1
        assert renames[0].old_name == "val"
        assert renames[0].new_name == "value"

    def test_no_column_renames(self, tmp_path: Path) -> None:
        """No renames when only columns are added."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, extra TEXT)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)
        renames = detect_column_renames(sd)
        assert renames == ()

    def test_multiple_column_renames(self, tmp_path: Path) -> None:
        """Multiple column renames in the same table are detected."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t "
                "(id INTEGER PRIMARY KEY, first_name TEXT, last_name TEXT)"
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t "
                "(id INTEGER PRIMARY KEY, given_name TEXT, family_name TEXT)"
            ],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        renames = detect_column_renames(sd)
        assert len(renames) == 2
        old_names = {r.old_name for r in renames}
        new_names = {r.new_name for r in renames}
        assert old_names == {"first_name", "last_name"}
        assert new_names == {"given_name", "family_name"}


# ---------------------------------------------------------------------------
# Type shift detection
# ---------------------------------------------------------------------------


class TestTypeShiftDetection:
    """Test type shift detection and compatibility classification."""

    def test_compatible_int_to_real(self, tmp_path: Path) -> None:
        """INT → REAL is compatible."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val INT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val REAL)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        shifts = detect_type_shifts(sd)
        assert len(shifts) == 1
        ts = shifts[0]
        assert ts.table_name == "t"
        assert ts.column_name == "val"
        assert ts.old_type == "INT"
        assert ts.new_type == "REAL"
        assert ts.old_affinity == "INTEGER"
        assert ts.new_affinity == "REAL"
        assert ts.compatible is True
        assert ts.confidence == 1.0

    def test_incompatible_text_to_int(self, tmp_path: Path) -> None:
        """TEXT → INTEGER is incompatible."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val INTEGER)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        shifts = detect_type_shifts(sd)
        assert len(shifts) == 1
        assert shifts[0].compatible is False

    def test_same_affinity_different_type(self, tmp_path: Path) -> None:
        """INT → BIGINT keeps INTEGER affinity — no shift detected since
        the affinity hasn't changed (both are INTEGER)."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val INT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val BIGINT)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        shifts = detect_type_shifts(sd)
        # The declared types differ but affinity is the same → still reported
        # as a compatible shift.
        assert len(shifts) == 1
        assert shifts[0].compatible is True

    def test_no_type_change(self, tmp_path: Path) -> None:
        """No shifts when only non-type properties change."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, val TEXT NOT NULL)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        shifts = detect_type_shifts(sd)
        assert shifts == ()

    def test_multiple_type_shifts(self, tmp_path: Path) -> None:
        """Multiple columns with type shifts are all detected."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b INT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, a INTEGER, b REAL)"],
        )

        ls = inspect_database(left)
        rs = inspect_database(right)
        sd = diff_schemas(ls, rs)

        shifts = detect_type_shifts(sd)
        assert len(shifts) == 2
        shift_map = {s.column_name: s for s in shifts}
        assert shift_map["a"].compatible is False  # TEXT → INTEGER
        assert shift_map["b"].compatible is True  # INTEGER → REAL


# ---------------------------------------------------------------------------
# Diff filtering
# ---------------------------------------------------------------------------


class TestDiffFiltering:
    """Test diff filtering by change type and table name."""

    def test_filter_by_change_type_added(self, tmp_path: Path) -> None:
        """Filter to only added changes."""
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

        result = diff_databases(left, right)
        filtered = filter_diff(result, change_types={"added"})

        # Only the added row should remain.
        td = filtered.table_data_diffs[0]
        assert td.rows_added == 1
        assert td.rows_removed == 0
        assert td.rows_modified == 0

    def test_filter_by_change_type_removed(self, tmp_path: Path) -> None:
        """Filter to only removed changes."""
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
                "INSERT INTO t VALUES (1, 'changed')",
            ],
        )

        result = diff_databases(left, right)
        filtered = filter_diff(result, change_types={"removed"})

        td = filtered.table_data_diffs[0]
        assert td.rows_removed == 1
        assert td.rows_added == 0
        assert td.rows_modified == 0

    def test_filter_by_table(self, tmp_path: Path) -> None:
        """Filter to a specific table."""
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
                "INSERT INTO a VALUES (1, 'changed')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'also_changed')",
            ],
        )

        result = diff_databases(left, right)
        filtered = filter_diff(result, tables={"a"})

        assert len(filtered.table_data_diffs) == 1
        assert filtered.table_data_diffs[0].table_name == "a"

    def test_filter_schema_by_change_type(self, tmp_path: Path) -> None:
        """Schema objects are filtered by change type."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
                "CREATE TABLE drop_me (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
                "CREATE TABLE new_one (id INTEGER PRIMARY KEY)",
            ],
        )

        result = diff_databases(left, right)
        filtered = filter_diff(result, change_types={"added"})

        assert len(filtered.schema_diff.tables_added) == 1
        assert len(filtered.schema_diff.tables_removed) == 0

    def test_filter_combined(self, tmp_path: Path) -> None:
        """Both change_type and table filters work together."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'old')",
                "INSERT INTO a VALUES (2, 'remove')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'y')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'new')",
                "INSERT INTO a VALUES (3, 'added')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'changed')",
            ],
        )

        result = diff_databases(left, right)
        filtered = filter_diff(result, change_types={"modified"}, tables={"a"})

        assert len(filtered.table_data_diffs) == 1
        td = filtered.table_data_diffs[0]
        assert td.table_name == "a"
        assert td.rows_modified == 1
        assert td.rows_added == 0
        assert td.rows_removed == 0


# ---------------------------------------------------------------------------
# Aggregate diff summary
# ---------------------------------------------------------------------------


class TestDiffSummary:
    """Test aggregate diff summary statistics."""

    def test_summary_empty_diff(self, tmp_path: Path) -> None:
        """Empty diff has all-zero summary."""
        stmts = ["CREATE TABLE t (id INTEGER PRIMARY KEY)"]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = diff_databases(left, right)
        summary = summarize_diff(result)

        assert summary.tables_added == 0
        assert summary.tables_removed == 0
        assert summary.tables_modified == 0
        assert summary.total_rows_added == 0
        assert summary.total_rows_removed == 0
        assert summary.total_rows_modified == 0
        assert summary.total_cell_changes == 0

    def test_summary_counts(self, tmp_path: Path) -> None:
        """Summary accurately counts all change types."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t1 VALUES (1, 'a')",
                "INSERT INTO t1 VALUES (2, 'b')",
                "INSERT INTO t1 VALUES (3, 'c')",
                "CREATE TABLE drop_me (id INTEGER PRIMARY KEY)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t1 (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t1 VALUES (1, 'changed')",
                "INSERT INTO t1 VALUES (4, 'new')",
                "CREATE TABLE added (id INTEGER PRIMARY KEY)",
            ],
        )

        result = diff_databases(left, right)
        summary = summarize_diff(result)

        assert summary.tables_added == 1
        assert summary.tables_removed == 1
        assert summary.total_rows_added == 1  # id=4
        assert summary.total_rows_removed == 2  # id=2, id=3
        assert summary.total_rows_modified == 1  # id=1
        assert summary.total_cell_changes >= 1  # at least the 'v' column

    def test_summary_cell_changes(self, tmp_path: Path) -> None:
        """Cell changes count multiple columns per row."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b INT)",
                "INSERT INTO t VALUES (1, 'old_a', 10)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b INT)",
                "INSERT INTO t VALUES (1, 'new_a', 20)",
            ],
        )

        result = diff_databases(left, right)
        summary = summarize_diff(result)

        assert summary.total_rows_modified == 1
        assert summary.total_cell_changes == 2  # Both 'a' and 'b' changed

    def test_summary_schema_objects(self, tmp_path: Path) -> None:
        """Summary counts indexes, triggers, views."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE INDEX idx_v ON t (v)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE VIEW v1 AS SELECT id FROM t",
            ],
        )

        result = diff_databases(left, right)
        summary = summarize_diff(result)

        assert summary.indexes_removed == 1
        assert summary.views_added == 1


# ---------------------------------------------------------------------------
# Annotations
# ---------------------------------------------------------------------------


class TestAnnotations:
    """Test diff annotations for triage workflows."""

    def test_add_annotation(self) -> None:
        annotations: tuple[DiffAnnotation, ...] = ()
        annotations = annotate(annotations, "table.users", "pending", "needs review")

        assert len(annotations) == 1
        assert annotations[0].target == "table.users"
        assert annotations[0].status == "pending"
        assert annotations[0].note == "needs review"

    def test_update_annotation(self) -> None:
        annotations: tuple[DiffAnnotation, ...] = ()
        annotations = annotate(annotations, "table.users", "pending")
        annotations = annotate(annotations, "table.users", "approved", "LGTM")

        assert len(annotations) == 1
        assert annotations[0].status == "approved"
        assert annotations[0].note == "LGTM"

    def test_multiple_targets(self) -> None:
        annotations: tuple[DiffAnnotation, ...] = ()
        annotations = annotate(annotations, "table.users", "pending")
        annotations = annotate(annotations, "table.orders", "approved")
        annotations = annotate(annotations, "table.users.column.email", "rejected")

        assert len(annotations) == 3
        targets = {a.target for a in annotations}
        assert targets == {"table.users", "table.orders", "table.users.column.email"}

    def test_annotation_statuses(self) -> None:
        """All valid statuses work."""
        annotations: tuple[DiffAnnotation, ...] = ()
        statuses = (
            "pending",
            "approved",
            "rejected",
            "needs-discussion",
            "deferred",
        )
        for status in statuses:
            annotations = annotate(
                annotations,
                f"t.{status}",
                status,  # type: ignore[arg-type]
            )

        assert len(annotations) == 5


# ---------------------------------------------------------------------------
# Full semantic analysis pipeline
# ---------------------------------------------------------------------------


class TestAnalyze:
    """Test the full ``analyze()`` pipeline."""

    def test_analyze_with_renames_and_shifts(self, tmp_path: Path) -> None:
        """Full analysis detects renames and type shifts."""
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE users "
                "(id INTEGER PRIMARY KEY, username TEXT NOT NULL, age INT)",
                "INSERT INTO users VALUES (1, 'alice', 30)",
                "CREATE TABLE logs (id INTEGER PRIMARY KEY, msg TEXT)",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE users "
                "(id INTEGER PRIMARY KEY, display_name TEXT NOT NULL, age REAL)",
                "INSERT INTO users VALUES (1, 'alice', 30)",
                "CREATE TABLE events (id INTEGER PRIMARY KEY, msg TEXT)",
            ],
        )

        result = diff_databases(left, right)
        analysis = analyze(result)

        # Table rename: logs → events
        assert len(analysis.table_renames) == 1
        assert analysis.table_renames[0].old_name == "logs"
        assert analysis.table_renames[0].new_name == "events"

        # Column rename: username → display_name
        assert len(analysis.column_renames) == 1
        assert analysis.column_renames[0].old_name == "username"
        assert analysis.column_renames[0].new_name == "display_name"

        # Type shift: age INT → REAL
        assert len(analysis.type_shifts) == 1
        assert analysis.type_shifts[0].column_name == "age"
        assert analysis.type_shifts[0].compatible is True

        # Summary should have counts.
        assert analysis.summary.tables_added == 1
        assert analysis.summary.tables_removed == 1

    def test_analyze_no_changes(self, tmp_path: Path) -> None:
        """Analysis on identical databases is empty."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = diff_databases(left, right)
        analysis = analyze(result)

        assert analysis.table_renames == ()
        assert analysis.column_renames == ()
        assert analysis.type_shifts == ()
        assert analysis.summary.total_rows_added == 0

    def test_analyze_thresholds(self, tmp_path: Path) -> None:
        """Custom thresholds affect detection sensitivity."""
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, old_col TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, new_col INT)"],
        )

        result = diff_databases(left, right)

        # High threshold should filter out the column rename.
        strict = analyze(result, column_rename_threshold=0.99)
        assert strict.column_renames == ()

        # Low threshold should catch it.
        loose = analyze(result, column_rename_threshold=0.5)
        assert len(loose.column_renames) == 1


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestSemanticEdgeCases:
    """Edge cases for semantic analysis."""

    def test_empty_databases(self, tmp_path: Path) -> None:
        """Semantic analysis on empty databases works."""
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = diff_databases(left, right)
        analysis = analyze(result)

        assert analysis.table_renames == ()
        assert analysis.column_renames == ()
        assert analysis.type_shifts == ()

    def test_filter_preserves_warnings(self, tmp_path: Path) -> None:
        """Filtering preserves warnings from the original diff."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = diff_databases(left, right)
        filtered = filter_diff(result, change_types={"added"})

        assert filtered.warnings == result.warnings

    def test_filter_nonexistent_table(self, tmp_path: Path) -> None:
        """Filtering to a nonexistent table produces empty results."""
        stmts = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO t VALUES (1, 'a')",
        ]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO t VALUES (1, 'b')",
            ],
        )

        result = diff_databases(left, right)
        filtered = filter_diff(result, tables={"nonexistent"})

        assert len(filtered.table_data_diffs) == 0

    def test_semantic_analysis_default_annotations(self, tmp_path: Path) -> None:
        """Default analysis has empty annotations."""
        stmts = ["CREATE TABLE t (id INTEGER PRIMARY KEY)"]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = diff_databases(left, right)
        analysis = analyze(result)

        assert analysis.annotations == ()

    def test_values_semantically_equal_blob(self) -> None:
        """Blob values are compared by identity."""
        assert values_semantically_equal(b"abc", b"abc") is True
        assert values_semantically_equal(b"abc", b"def") is False

    def test_values_semantically_equal_bool(self) -> None:
        """Booleans work with numeric comparison."""
        # In Python, bool is a subclass of int.
        assert values_semantically_equal(True, 1) is True
        assert values_semantically_equal(False, 0) is True
        assert values_semantically_equal(True, 1.0) is True

    def test_type_affinity_empty_string(self) -> None:
        """Empty type string gets BLOB affinity."""
        assert sqlite_type_affinity("") == "BLOB"

    def test_type_affinity_whitespace(self) -> None:
        """Whitespace-only type string gets BLOB affinity."""
        assert sqlite_type_affinity("   ") == "BLOB"
