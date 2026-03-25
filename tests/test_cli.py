"""Tests for the patchworks CLI (Phase 5 + existing smoke tests)."""

from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

import patchworks

_TABLE = "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"
_TABLE_BARE = "CREATE TABLE t (id INTEGER PRIMARY KEY)"


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    """Run ``python -m patchworks`` with the given arguments."""
    return subprocess.run(
        [sys.executable, "-m", "patchworks", *args],
        capture_output=True,
        text=True,
    )


def _create_db(path: Path, statements: list[str] | None = None) -> Path:
    conn = sqlite3.connect(str(path))
    for stmt in statements or []:
        conn.execute(stmt)
    conn.commit()
    conn.close()
    return path


# ---------------------------------------------------------------------------
# Smoke tests (Phase 0/1)
# ---------------------------------------------------------------------------


class TestSmoke:
    def test_version_string(self) -> None:
        assert patchworks.__version__ == "0.1.0"

    def test_help_exits_zero(self) -> None:
        result = _run("--help")
        assert result.returncode == 0
        assert "patchworks" in result.stdout

    def test_help_shows_all_subcommands(self) -> None:
        result = _run("--help")
        for cmd in (
            "inspect",
            "diff",
            "export",
            "snapshot",
            "merge",
            "migrate",
            "serve",
        ):
            assert cmd in result.stdout, f"missing: {cmd}"

    def test_version_flag(self) -> None:
        result = _run("--version")
        assert result.returncode == 0
        assert "0.1.0" in result.stdout

    def test_no_args_shows_help(self) -> None:
        result = _run()
        assert result.returncode == 0
        assert "patchworks" in result.stdout


# ---------------------------------------------------------------------------
# inspect
# ---------------------------------------------------------------------------


class TestInspectCLI:
    def test_inspect_human(self, tmp_path: Path) -> None:
        db = _create_db(
            tmp_path / "test.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        result = _run("inspect", str(db))
        assert result.returncode == 0
        assert "Database:" in result.stdout
        assert "Table: t" in result.stdout

    def test_inspect_json(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db", [_TABLE_BARE])
        result = _run("inspect", str(db), "--format", "json")
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert "tables" in data
        assert data["tables"][0]["name"] == "t"

    def test_inspect_missing_db(self, tmp_path: Path) -> None:
        result = _run("inspect", str(tmp_path / "nope.db"))
        assert result.returncode == 1
        assert "error" in result.stderr


# ---------------------------------------------------------------------------
# diff
# ---------------------------------------------------------------------------


class TestDiffCLI:
    def test_diff_no_changes(self, tmp_path: Path) -> None:
        stmts = [_TABLE, "INSERT INTO t VALUES (1, 'a')"]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = _run("diff", str(left), str(right))
        assert result.returncode == 0
        assert "No differences" in result.stdout

    def test_diff_with_changes(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'b')"],
        )

        result = _run("diff", str(left), str(right))
        assert result.returncode == 2  # EXIT_DIFFERENCES
        assert "Table t" in result.stdout

    def test_diff_json(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'b')"],
        )

        result = _run(
            "diff",
            str(left),
            str(right),
            "--format",
            "json",
        )
        assert result.returncode == 2
        data = json.loads(result.stdout)
        assert data["has_changes"] is True

    def test_diff_missing_db(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "left.db", [])
        result = _run("diff", str(db), str(tmp_path / "nope.db"))
        assert result.returncode == 1

    def test_diff_schema_changes(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE_BARE],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE],
        )

        result = _run("diff", str(left), str(right))
        assert result.returncode == 2
        assert "Schema changes" in result.stdout


# ---------------------------------------------------------------------------
# export
# ---------------------------------------------------------------------------


class TestExportCLI:
    def test_export_stdout(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'b')"],
        )

        result = _run("export", str(left), str(right))
        assert result.returncode == 0
        assert "PRAGMA foreign_keys" in result.stdout
        assert "UPDATE" in result.stdout

    def test_export_to_file(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'b')"],
        )
        out = tmp_path / "migration.sql"

        result = _run(
            "export",
            str(left),
            str(right),
            "-o",
            str(out),
        )
        assert result.returncode == 0
        assert out.exists()
        content = out.read_text()
        assert "UPDATE" in content

    def test_export_missing_db(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "left.db", [])
        result = _run(
            "export",
            str(db),
            str(tmp_path / "nope.db"),
        )
        assert result.returncode == 1


# ---------------------------------------------------------------------------
# snapshot
# ---------------------------------------------------------------------------


class TestSnapshotCLI:
    def test_snapshot_save(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db", [_TABLE_BARE])
        result = _run("snapshot", "save", str(db))
        assert result.returncode == 0
        assert "Snapshot saved:" in result.stdout

    def test_snapshot_save_with_name(self, tmp_path: Path) -> None:
        db = _create_db(tmp_path / "test.db")
        result = _run(
            "snapshot",
            "save",
            str(db),
            "--name",
            "my-snap",
        )
        assert result.returncode == 0
        assert "my-snap" in result.stdout

    def test_snapshot_list_human(self, tmp_path: Path) -> None:
        result = _run("snapshot", "list")
        assert result.returncode == 0

    def test_snapshot_list_json(self, tmp_path: Path) -> None:
        result = _run("snapshot", "list", "--format", "json")
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert isinstance(data, list)

    def test_snapshot_delete_nonexistent(
        self,
        tmp_path: Path,
    ) -> None:
        result = _run("snapshot", "delete", "nonexistent-uuid")
        assert result.returncode == 1

    def test_snapshot_save_missing_db(
        self,
        tmp_path: Path,
    ) -> None:
        result = _run(
            "snapshot",
            "save",
            str(tmp_path / "nope.db"),
        )
        assert result.returncode == 1

    def test_snapshot_no_subcommand(self, tmp_path: Path) -> None:
        result = _run("snapshot")
        assert result.returncode == 1


# ---------------------------------------------------------------------------
# Exit codes
# ---------------------------------------------------------------------------


class TestExitCodes:
    def test_exit_0_no_differences(self, tmp_path: Path) -> None:
        stmts = [_TABLE_BARE]
        left = _create_db(tmp_path / "left.db", stmts)
        right = _create_db(tmp_path / "right.db", stmts)

        result = _run("diff", str(left), str(right))
        assert result.returncode == 0

    def test_exit_1_error(self, tmp_path: Path) -> None:
        result = _run("inspect", str(tmp_path / "nope.db"))
        assert result.returncode == 1

    def test_exit_2_differences(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE_BARE],
        )
        right = _create_db(
            tmp_path / "right.db",
            [
                _TABLE_BARE,
                "CREATE TABLE t2 (id INTEGER PRIMARY KEY)",
            ],
        )

        result = _run("diff", str(left), str(right))
        assert result.returncode == 2
