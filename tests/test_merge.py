"""Comprehensive tests for Phase 7 — three-way merge engine.

Covers:
- Non-conflicting merges (rows and schema)
- Row conflicts (same row modified differently)
- Schema conflicts (same table modified differently)
- Delete-modify conflicts (row or table)
- Table-delete conflicts
- Edge cases (empty databases, identical changes, etc.)
- CLI merge subcommand
"""

from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

from patchworks.diff.merge import merge_databases

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


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-m", "patchworks", *args],
        capture_output=True,
        text=True,
    )


_TABLE = "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"


# ---------------------------------------------------------------------------
# Non-conflicting merges
# ---------------------------------------------------------------------------


class TestNonConflictingMerge:
    """Tests for clean (non-conflicting) three-way merges."""

    def test_no_changes_on_either_side(self, tmp_path: Path) -> None:
        """Identical databases produce a clean merge with nothing to do."""
        stmts = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", stmts)
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert len(result.conflicts) == 0
        assert len(result.merged_rows) == 0
        assert len(result.merged_schema) == 0

    def test_left_adds_row(self, tmp_path: Path) -> None:
        """Left adds a row, right unchanged — clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "INSERT INTO t VALUES (2, 'b')"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.table == "t" and mr.kind == "added" and mr.source == "left"
            for mr in result.merged_rows
        )

    def test_right_adds_row(self, tmp_path: Path) -> None:
        """Right adds a row, left unchanged — clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(tmp_path / "left.db", base)
        right = _create_db(
            tmp_path / "right.db",
            [*base, "INSERT INTO t VALUES (2, 'b')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.table == "t" and mr.kind == "added" and mr.source == "right"
            for mr in result.merged_rows
        )

    def test_both_add_different_rows(self, tmp_path: Path) -> None:
        """Both sides add different rows — clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "INSERT INTO t VALUES (2, 'left')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [*base, "INSERT INTO t VALUES (3, 'right')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        added_keys = {mr.key for mr in result.merged_rows if mr.kind == "added"}
        assert (2,) in added_keys
        assert (3,) in added_keys

    def test_left_removes_row(self, tmp_path: Path) -> None:
        """Left removes a row, right unchanged — clean merge."""
        base = [
            _TABLE,
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.table == "t" and mr.kind == "removed" and mr.source == "left"
            for mr in result.merged_rows
        )

    def test_both_remove_same_row(self, tmp_path: Path) -> None:
        """Both sides remove the same row — clean merge."""
        base = [
            _TABLE,
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.kind == "removed" and mr.source == "both" for mr in result.merged_rows
        )

    def test_left_modifies_right_unchanged(self, tmp_path: Path) -> None:
        """Left modifies a row, right unchanged — clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'changed')"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.table == "t" and mr.kind == "modified" and mr.source == "left"
            for mr in result.merged_rows
        )

    def test_both_modify_different_columns(self, tmp_path: Path) -> None:
        """Both sides modify different columns of the same row — clean merge."""
        table = "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT)"
        base = [table, "INSERT INTO t VALUES (1, 'old_a', 'old_b')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [table, "INSERT INTO t VALUES (1, 'new_a', 'old_b')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [table, "INSERT INTO t VALUES (1, 'old_a', 'new_b')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        merged_mod = [mr for mr in result.merged_rows if mr.kind == "modified"]
        assert len(merged_mod) == 1
        assert merged_mod[0].values is not None
        assert merged_mod[0].values["a"] == "new_a"
        assert merged_mod[0].values["b"] == "new_b"

    def test_both_modify_same_column_same_value(self, tmp_path: Path) -> None:
        """Both sides change the same column to the same value — clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'old')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'new')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'new')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean

    def test_left_adds_table(self, tmp_path: Path) -> None:
        """Left adds a new table — clean schema merge."""
        base = ["CREATE TABLE t1 (id INTEGER PRIMARY KEY)"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT)"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "t2" and ms.kind == "added" and ms.source == "left"
            for ms in result.merged_schema
        )

    def test_both_add_identical_table(self, tmp_path: Path) -> None:
        """Both sides add the same table with identical schema — clean merge."""
        base = ["CREATE TABLE t1 (id INTEGER PRIMARY KEY)"]
        new_table = "CREATE TABLE t2 (id INTEGER PRIMARY KEY, name TEXT)"
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(tmp_path / "left.db", [*base, new_table])
        right = _create_db(tmp_path / "right.db", [*base, new_table])

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "t2" and ms.kind == "added" and ms.source == "both"
            for ms in result.merged_schema
        )


# ---------------------------------------------------------------------------
# Row conflicts
# ---------------------------------------------------------------------------


class TestRowConflicts:
    """Tests for row-level merge conflicts."""

    def test_same_row_modified_differently(self, tmp_path: Path) -> None:
        """Both sides modify the same column to different values — conflict."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'original')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'left_change')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'right_change')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "row" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "row")
        assert conflict.table == "t"
        assert conflict.key == (1,)

    def test_same_row_added_differently(self, tmp_path: Path) -> None:
        """Both sides add a row with the same key but different values — conflict."""
        base = [_TABLE]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'left_val')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'right_val')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "row" for c in result.conflicts)

    def test_both_add_same_row_identical(self, tmp_path: Path) -> None:
        """Both sides add the same row with identical values — clean."""
        base = [_TABLE]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'same')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'same')"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            mr.kind == "added" and mr.source == "both" for mr in result.merged_rows
        )

    def test_multiple_row_conflicts(self, tmp_path: Path) -> None:
        """Multiple rows conflicting in the same table."""
        base = [
            _TABLE,
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [
                _TABLE,
                "INSERT INTO t VALUES (1, 'l1')",
                "INSERT INTO t VALUES (2, 'l2')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                _TABLE,
                "INSERT INTO t VALUES (1, 'r1')",
                "INSERT INTO t VALUES (2, 'r2')",
            ],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        row_conflicts = [c for c in result.conflicts if c.kind == "row"]
        assert len(row_conflicts) == 2


# ---------------------------------------------------------------------------
# Schema conflicts
# ---------------------------------------------------------------------------


class TestSchemaConflicts:
    """Tests for schema-level merge conflicts."""

    def test_same_table_modified_differently(self, tmp_path: Path) -> None:
        """Both sides modify the same table's schema differently — conflict."""
        base = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, left_col INT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, right_col REAL)"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "schema" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "schema")
        assert conflict.table == "t"

    def test_both_add_table_different_schema(self, tmp_path: Path) -> None:
        """Both add a table with the same name, different schemas."""
        base = ["CREATE TABLE existing (id INTEGER PRIMARY KEY)"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "CREATE TABLE new_t (id INTEGER PRIMARY KEY, a TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [*base, "CREATE TABLE new_t (id INTEGER PRIMARY KEY, b INT)"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "schema" and c.table == "new_t" for c in result.conflicts)

    def test_same_table_modified_identically(self, tmp_path: Path) -> None:
        """Both sides modify a table the same way — clean merge."""
        base = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]
        modified = "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, extra INT)"
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(tmp_path / "left.db", [modified])
        right = _create_db(tmp_path / "right.db", [modified])

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "t" and ms.kind == "modified" and ms.source == "both"
            for ms in result.merged_schema
        )


# ---------------------------------------------------------------------------
# Delete-modify conflicts
# ---------------------------------------------------------------------------


class TestDeleteModifyConflicts:
    """Tests for delete-modify conflicts (row level)."""

    def test_left_deletes_right_modifies(self, tmp_path: Path) -> None:
        """Left deletes a row, right modifies it — conflict."""
        base = [
            _TABLE,
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],  # removed id=2
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                _TABLE,
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'changed')",  # modified id=2
            ],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "delete-modify" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "delete-modify")
        assert conflict.key == (2,)
        assert "deleted on left" in conflict.description

    def test_right_deletes_left_modifies(self, tmp_path: Path) -> None:
        """Right deletes a row, left modifies it — conflict."""
        base = [
            _TABLE,
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [
                _TABLE,
                "INSERT INTO t VALUES (1, 'a')",
                "INSERT INTO t VALUES (2, 'modified')",  # modified id=2
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],  # removed id=2
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "delete-modify" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "delete-modify")
        assert "deleted on right" in conflict.description


# ---------------------------------------------------------------------------
# Table-delete conflicts
# ---------------------------------------------------------------------------


class TestTableDeleteConflicts:
    """Tests for table-delete conflicts (schema level)."""

    def test_left_drops_right_modifies_schema(self, tmp_path: Path) -> None:
        """Left drops a table, right modifies its schema — conflict."""
        base = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(tmp_path / "left.db", [])  # dropped t
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, extra INT)"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "table-delete" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "table-delete")
        assert conflict.table == "t"
        assert "dropped on left" in conflict.description

    def test_right_drops_left_modifies_schema(self, tmp_path: Path) -> None:
        """Right drops a table, left modifies its schema — conflict."""
        base = ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, extra INT)"],
        )
        right = _create_db(tmp_path / "right.db", [])  # dropped t

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        assert any(c.kind == "table-delete" for c in result.conflicts)
        conflict = next(c for c in result.conflicts if c.kind == "table-delete")
        assert "dropped on right" in conflict.description

    def test_both_drop_same_table(self, tmp_path: Path) -> None:
        """Both sides drop the same table — clean merge."""
        base = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE keep (id INTEGER PRIMARY KEY)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE keep (id INTEGER PRIMARY KEY)"],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "t" and ms.kind == "removed" and ms.source == "both"
            for ms in result.merged_schema
        )

    def test_left_drops_table_right_unchanged(self, tmp_path: Path) -> None:
        """Left drops a table, right leaves it untouched — clean merge."""
        base = [
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            "CREATE TABLE keep (id INTEGER PRIMARY KEY)",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE keep (id INTEGER PRIMARY KEY)"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "t" and ms.kind == "removed" and ms.source == "left"
            for ms in result.merged_schema
        )


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestMergeEdgeCases:
    """Edge cases for the merge engine."""

    def test_all_empty_databases(self, tmp_path: Path) -> None:
        """Three empty databases produce a clean merge."""
        ancestor = _create_db(tmp_path / "ancestor.db", [])
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert len(result.conflicts) == 0

    def test_mixed_conflicts_and_clean(self, tmp_path: Path) -> None:
        """Some rows conflict while others merge cleanly."""
        table = "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT)"
        base = [
            table,
            "INSERT INTO t VALUES (1, 'x', 'y')",
            "INSERT INTO t VALUES (2, 'p', 'q')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [
                table,
                "INSERT INTO t VALUES (1, 'left_x', 'y')",  # modify col a
                "INSERT INTO t VALUES (2, 'p', 'q')",  # unchanged
                "INSERT INTO t VALUES (3, 'new', 'row')",  # added
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                table,
                "INSERT INTO t VALUES (1, 'right_x', 'y')",  # conflict on col a
                "INSERT INTO t VALUES (2, 'p', 'changed_q')",  # modify col b
            ],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.has_conflicts
        # One conflict on row 1 (both modify col a differently).
        row_conflicts = [c for c in result.conflicts if c.kind == "row"]
        assert len(row_conflicts) == 1
        assert row_conflicts[0].key == (1,)

        # Clean merges: row 2 modified on right, row 3 added on left.
        assert any(
            mr.key == (2,) and mr.kind == "modified" for mr in result.merged_rows
        )
        assert any(
            mr.key == (3,) and mr.kind == "added" and mr.source == "left"
            for mr in result.merged_rows
        )

    def test_multiple_tables_independent_changes(self, tmp_path: Path) -> None:
        """Changes in different tables merge independently."""
        base = [
            "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO a VALUES (1, 'a1')",
            "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO b VALUES (1, 'b1')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'left_a1')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'b1')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO a VALUES (1, 'a1')",
                "CREATE TABLE b (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO b VALUES (1, 'right_b1')",
            ],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(mr.table == "a" and mr.source == "left" for mr in result.merged_rows)
        assert any(
            mr.table == "b" and mr.source == "right" for mr in result.merged_rows
        )

    def test_merge_result_properties(self, tmp_path: Path) -> None:
        """MergeResult properties work correctly."""
        stmts = [_TABLE]
        ancestor = _create_db(tmp_path / "ancestor.db", stmts)
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean is True
        assert result.has_conflicts is False
        assert result.ancestor_path == str(tmp_path / "ancestor.db")
        assert result.left_path == str(tmp_path / "left.db")
        assert result.right_path == str(tmp_path / "right.db")

    def test_schema_and_row_changes_combined(self, tmp_path: Path) -> None:
        """Schema changes in one table with row changes in another."""
        base = [
            "CREATE TABLE schema_t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE TABLE data_t (id INTEGER PRIMARY KEY, v TEXT)",
            "INSERT INTO data_t VALUES (1, 'old')",
        ]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [
                "CREATE TABLE schema_t (id INTEGER PRIMARY KEY, v TEXT, extra INT)",
                "CREATE TABLE data_t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO data_t VALUES (1, 'old')",
            ],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                "CREATE TABLE schema_t (id INTEGER PRIMARY KEY, v TEXT)",
                "CREATE TABLE data_t (id INTEGER PRIMARY KEY, v TEXT)",
                "INSERT INTO data_t VALUES (1, 'new')",
            ],
        )

        result = merge_databases(str(ancestor), str(left), str(right))

        assert result.is_clean
        assert any(
            ms.table == "schema_t" and ms.kind == "modified" and ms.source == "left"
            for ms in result.merged_schema
        )
        assert any(
            mr.table == "data_t" and mr.kind == "modified" and mr.source == "right"
            for mr in result.merged_rows
        )


# ---------------------------------------------------------------------------
# CLI merge subcommand
# ---------------------------------------------------------------------------


class TestMergeCLI:
    """Tests for the `patchworks merge` CLI subcommand."""

    def test_merge_human_clean(self, tmp_path: Path) -> None:
        """Human output for a clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "INSERT INTO t VALUES (2, 'b')"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = _run(
            "merge",
            str(ancestor),
            str(left),
            str(right),
        )

        assert result.returncode == 0
        assert "clean" in result.stdout.lower() or "No conflicts" in result.stdout

    def test_merge_human_conflict(self, tmp_path: Path) -> None:
        """Human output shows conflicts."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'original')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'left')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'right')"],
        )

        result = _run(
            "merge",
            str(ancestor),
            str(left),
            str(right),
        )

        assert result.returncode == 2  # EXIT_DIFFERENCES for conflicts
        assert "CONFLICT" in result.stdout or "conflict" in result.stdout.lower()

    def test_merge_json_clean(self, tmp_path: Path) -> None:
        """JSON output for a clean merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(tmp_path / "left.db", base)
        right = _create_db(tmp_path / "right.db", base)

        result = _run(
            "merge",
            str(ancestor),
            str(left),
            str(right),
            "--format",
            "json",
        )

        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert data["is_clean"] is True
        assert data["conflicts"] == []

    def test_merge_json_conflict(self, tmp_path: Path) -> None:
        """JSON output for a conflicting merge."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'original')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'left')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'right')"],
        )

        result = _run(
            "merge",
            str(ancestor),
            str(left),
            str(right),
            "--format",
            "json",
        )

        assert result.returncode == 2
        data = json.loads(result.stdout)
        assert data["is_clean"] is False
        assert len(data["conflicts"]) >= 1
        assert data["conflicts"][0]["kind"] == "row"

    def test_merge_missing_ancestor(self, tmp_path: Path) -> None:
        """Missing ancestor database returns error."""
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = _run(
            "merge",
            str(tmp_path / "nope.db"),
            str(left),
            str(right),
        )

        assert result.returncode == 1

    def test_merge_missing_left(self, tmp_path: Path) -> None:
        """Missing left database returns error."""
        ancestor = _create_db(tmp_path / "ancestor.db", [])
        right = _create_db(tmp_path / "right.db", [])

        result = _run(
            "merge",
            str(ancestor),
            str(tmp_path / "nope.db"),
            str(right),
        )

        assert result.returncode == 1

    def test_merge_json_has_expected_structure(self, tmp_path: Path) -> None:
        """JSON output has all expected keys."""
        base = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        ancestor = _create_db(tmp_path / "ancestor.db", base)
        left = _create_db(
            tmp_path / "left.db",
            [*base, "INSERT INTO t VALUES (2, 'added')"],
        )
        right = _create_db(tmp_path / "right.db", base)

        result = _run(
            "merge",
            str(ancestor),
            str(left),
            str(right),
            "--format",
            "json",
        )

        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert "ancestor_path" in data
        assert "left_path" in data
        assert "right_path" in data
        assert "is_clean" in data
        assert "conflicts" in data
        assert "merged_rows" in data
        assert "merged_schema" in data
        assert isinstance(data["merged_rows"], list)
        assert len(data["merged_rows"]) > 0
        row = data["merged_rows"][0]
        assert "table" in row
        assert "kind" in row
        assert "source" in row
        assert "key" in row
