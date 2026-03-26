"""Comprehensive tests for Phase 8 — migration workflow management.

Covers:
- MigrationStore: save, list, get, delete, mark_applied, mark_unapplied,
  sequence ordering, squash replacement
- Migration generation (forward + reverse SQL)
- Migration validation (apply to temp copy, diff against target)
- Migration apply and rollback
- Squashing sequential migrations
- Conflict detection between migrations
- Dry-run modes (generate, apply, squash)
- CLI migrate subcommand family
"""

from __future__ import annotations

import json
import sqlite3
import subprocess
import sys
from typing import TYPE_CHECKING

import pytest

from patchworks.db.migration import MigrationStore
from patchworks.diff.migration import (
    apply_migration,
    detect_conflicts,
    generate_migration,
    squash_migrations,
    validate_migration,
)

if TYPE_CHECKING:
    from pathlib import Path

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
# MigrationStore tests
# ---------------------------------------------------------------------------


class TestMigrationStore:
    """Tests for the MigrationStore persistence layer."""

    def test_save_and_get(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="SELECT 1;",
            reverse_sql="SELECT 2;",
            name="test-mig",
        )

        assert mig.id
        assert mig.name == "test-mig"
        assert mig.sequence == 1
        assert mig.forward_sql == "SELECT 1;"
        assert mig.reverse_sql == "SELECT 2;"
        assert not mig.applied
        assert mig.applied_at is None

        fetched = store.get(mig.id)
        assert fetched is not None
        assert fetched.id == mig.id
        assert fetched.name == "test-mig"

    def test_get_nonexistent(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        assert store.get("nonexistent") is None

    def test_list_empty(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        assert store.list() == []

    def test_list_ordered_by_sequence(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="1",
            reverse_sql="1",
        )
        m2 = store.save(
            source_path="/b.db",
            target_path="/c.db",
            forward_sql="2",
            reverse_sql="2",
        )

        result = store.list()
        assert len(result) == 2
        assert result[0].id == m1.id
        assert result[1].id == m2.id
        assert result[0].sequence < result[1].sequence

    def test_delete(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
        )

        assert store.delete(mig.id) is True
        assert store.get(mig.id) is None
        assert store.delete(mig.id) is False

    def test_mark_applied(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
        )

        assert store.mark_applied(mig.id) is True
        fetched = store.get(mig.id)
        assert fetched is not None
        assert fetched.applied is True
        assert fetched.applied_at is not None

    def test_mark_unapplied(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
        )

        store.mark_applied(mig.id)
        store.mark_unapplied(mig.id)
        fetched = store.get(mig.id)
        assert fetched is not None
        assert fetched.applied is False
        assert fetched.applied_at is None

    def test_sequence_auto_increments(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        seqs: list[int] = []
        for i in range(5):
            m = store.save(
                source_path="/a.db",
                target_path="/b.db",
                forward_sql=str(i),
                reverse_sql=str(i),
            )
            seqs.append(m.sequence)

        assert seqs == [1, 2, 3, 4, 5]

    def test_objects_touched(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users", "index:idx_name"],
        )

        assert "index:idx_name" in mig.objects_touched
        assert "table:users" in mig.objects_touched

    def test_get_by_sequence_range(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        for i in range(5):
            store.save(
                source_path="/a.db",
                target_path="/b.db",
                forward_sql=str(i),
                reverse_sql=str(i),
            )

        result = store.get_by_sequence_range(2, 4)
        assert len(result) == 3
        assert [m.sequence for m in result] == [2, 3, 4]

    def test_replace_with_squashed(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="f1",
            reverse_sql="r1",
        )
        m2 = store.save(
            source_path="/b.db",
            target_path="/c.db",
            forward_sql="f2",
            reverse_sql="r2",
        )

        squashed = store.replace_with_squashed(
            [m1.id, m2.id],
            source_path="/a.db",
            target_path="/c.db",
            forward_sql="f1\nf2",
            reverse_sql="r2\nr1",
            name="squashed",
        )

        assert squashed.sequence == 1  # takes the lowest
        assert store.get(m1.id) is None
        assert store.get(m2.id) is None
        assert store.get(squashed.id) is not None
        all_migs = store.list()
        assert len(all_migs) == 1

    def test_creates_directories(self, tmp_path: Path) -> None:
        base = tmp_path / "deep" / "nested" / "store"
        MigrationStore(base_dir=base)
        assert base.exists()
        assert (base / "patchworks.db").exists()


# ---------------------------------------------------------------------------
# Migration generation tests
# ---------------------------------------------------------------------------


class TestMigrationGeneration:
    """Tests for generate_migration."""

    def test_generate_basic(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')", "INSERT INTO t VALUES (2, 'b')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        result = generate_migration(left, right, store, name="add-row")

        assert result.migration is not None
        assert result.migration.name == "add-row"
        assert "INSERT" in result.forward_sql
        assert result.reverse_sql  # non-empty
        assert not result.dry_run

    def test_generate_dry_run(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        result = generate_migration(left, right, store, dry_run=True)

        assert result.migration is None
        assert result.dry_run is True
        assert result.forward_sql  # still generated
        # Store should be empty.
        assert store.list() == []

    def test_generate_captures_objects_touched(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        result = generate_migration(left, right, store)

        assert any("table:t" in o for o in result.objects_touched)

    def test_generate_schema_change(self, tmp_path: Path) -> None:
        left = _create_db(
            tmp_path / "left.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)"],
        )
        right = _create_db(
            tmp_path / "right.db",
            ["CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT, extra INT)"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        result = generate_migration(left, right, store)

        assert result.migration is not None
        assert "table:t" in result.objects_touched


# ---------------------------------------------------------------------------
# Migration validation tests
# ---------------------------------------------------------------------------


class TestMigrationValidation:
    """Tests for validate_migration."""

    def test_valid_migration(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        result = generate_migration(left, right, store)
        assert result.migration is not None

        vr = validate_migration(result.migration)

        assert vr.valid is True
        assert "valid" in vr.message

    def test_invalid_when_source_missing(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path=str(tmp_path / "missing.db"),
            target_path=str(tmp_path / "also_missing.db"),
            forward_sql="SELECT 1;",
            reverse_sql="SELECT 1;",
        )

        vr = validate_migration(mig)

        assert vr.valid is False
        assert "not found" in vr.message

    def test_invalid_sql(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        # Manually save with broken SQL.
        mig = store.save(
            source_path=str(left.resolve()),
            target_path=str(right.resolve()),
            forward_sql="THIS IS NOT SQL;",
            reverse_sql="ALSO NOT SQL;",
        )

        vr = validate_migration(mig)

        assert vr.valid is False
        assert "failed" in vr.message


# ---------------------------------------------------------------------------
# Migration apply/rollback tests
# ---------------------------------------------------------------------------


class TestMigrationApply:
    """Tests for apply_migration."""

    def test_apply_forward(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        gen = generate_migration(left, right, store)
        assert gen.migration is not None

        # Apply to a copy of left.
        target = tmp_path / "target.db"
        _create_db(target, [_TABLE])

        result = apply_migration(gen.migration, target, store)

        assert result.success is True
        # Verify the row was inserted.
        conn = sqlite3.connect(str(target))
        rows = conn.execute("SELECT v FROM t WHERE id = 1").fetchall()
        conn.close()
        assert rows[0][0] == "hello"

        # Migration should be marked applied.
        fetched = store.get(gen.migration.id)
        assert fetched is not None
        assert fetched.applied is True

    def test_apply_rollback(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        gen = generate_migration(left, right, store)
        assert gen.migration is not None

        # Apply forward first.
        target = tmp_path / "target.db"
        _create_db(target, [_TABLE])
        apply_migration(gen.migration, target, store)

        # Rollback.
        result = apply_migration(gen.migration, target, store, rollback=True)

        assert result.success is True
        # Row should be gone.
        conn = sqlite3.connect(str(target))
        rows = conn.execute("SELECT COUNT(*) FROM t").fetchall()
        conn.close()
        assert rows[0][0] == 0

        # Migration should be marked unapplied.
        fetched = store.get(gen.migration.id)
        assert fetched is not None
        assert fetched.applied is False

    def test_apply_dry_run(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        gen = generate_migration(left, right, store)
        assert gen.migration is not None

        target = tmp_path / "target.db"
        _create_db(target, [_TABLE])

        result = apply_migration(gen.migration, target, store, dry_run=True)

        assert result.success is True
        assert result.dry_run is True
        # Target should be unchanged.
        conn = sqlite3.connect(str(target))
        rows = conn.execute("SELECT COUNT(*) FROM t").fetchall()
        conn.close()
        assert rows[0][0] == 0

        # Migration should NOT be marked applied.
        fetched = store.get(gen.migration.id)
        assert fetched is not None
        assert fetched.applied is False

    def test_apply_target_not_found(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        mig = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="SELECT 1;",
            reverse_sql="SELECT 1;",
        )

        result = apply_migration(mig, tmp_path / "nope.db", store)

        assert result.success is False
        assert "not found" in result.message


# ---------------------------------------------------------------------------
# Squash tests
# ---------------------------------------------------------------------------


class TestSquash:
    """Tests for squash_migrations."""

    def test_squash_two(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "v1.db", [_TABLE])
        mid = _create_db(
            tmp_path / "v2.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        right = _create_db(
            tmp_path / "v3.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')", "INSERT INTO t VALUES (2, 'b')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        g1 = generate_migration(left, mid, store, name="m1")
        g2 = generate_migration(mid, right, store, name="m2")
        assert g1.migration is not None
        assert g2.migration is not None

        result = squash_migrations(
            [g1.migration, g2.migration],
            store,
            name="squashed",
        )

        assert result.migration is not None
        assert result.squashed_count == 2
        assert result.migration.name == "squashed"
        assert not result.dry_run
        # Old migrations gone, new one exists.
        all_migs = store.list()
        assert len(all_migs) == 1
        assert all_migs[0].id == result.migration.id

    def test_squash_dry_run(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="f1",
            reverse_sql="r1",
        )
        m2 = store.save(
            source_path="/b.db",
            target_path="/c.db",
            forward_sql="f2",
            reverse_sql="r2",
        )

        result = squash_migrations([m1, m2], store, dry_run=True)

        assert result.dry_run is True
        assert result.migration is None
        assert result.squashed_count == 2
        # Store should still have both.
        assert len(store.list()) == 2

    def test_squash_requires_minimum_two(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="f1",
            reverse_sql="r1",
        )

        with pytest.raises(ValueError, match="at least 2"):
            squash_migrations([m1], store)

    def test_squash_requires_sequential(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="f1",
            reverse_sql="r1",
        )
        _m2 = store.save(
            source_path="/b.db",
            target_path="/c.db",
            forward_sql="f2",
            reverse_sql="r2",
        )
        m3 = store.save(
            source_path="/c.db",
            target_path="/d.db",
            forward_sql="f3",
            reverse_sql="r3",
        )

        with pytest.raises(ValueError, match="not sequential"):
            squash_migrations([m1, m3], store)

    def test_squash_reverse_sql_in_reverse_order(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="forward_1",
            reverse_sql="reverse_1",
        )
        m2 = store.save(
            source_path="/b.db",
            target_path="/c.db",
            forward_sql="forward_2",
            reverse_sql="reverse_2",
        )

        result = squash_migrations([m1, m2], store, dry_run=True)

        # Forward is in order, reverse is reversed.
        assert "forward_1" in result.forward_sql
        fwd = result.forward_sql
        assert fwd.index("forward_1") < fwd.index("forward_2")
        assert "reverse_2" in result.reverse_sql
        rev = result.reverse_sql
        assert rev.index("reverse_2") < rev.index("reverse_1")


# ---------------------------------------------------------------------------
# Conflict detection tests
# ---------------------------------------------------------------------------


class TestConflictDetection:
    """Tests for detect_conflicts."""

    def test_no_conflicts(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users"],
        )
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:orders"],
        )

        report = detect_conflicts(store)

        assert not report.has_conflicts

    def test_conflict_detected(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users", "index:idx_email"],
        )
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users", "table:orders"],
        )

        report = detect_conflicts(store)

        assert report.has_conflicts
        assert len(report.conflicts) == 1
        assert "table:users" in report.conflicts[0].shared_objects

    def test_applied_migrations_excluded(self, tmp_path: Path) -> None:
        """Applied migrations should not be checked for conflicts."""
        store = MigrationStore(base_dir=tmp_path / "store")
        m1 = store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users"],
        )
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:users"],
        )

        store.mark_applied(m1.id)
        report = detect_conflicts(store)

        # Only one pending migration left, no pair to conflict.
        assert not report.has_conflicts

    def test_multiple_conflicts(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:a"],
        )
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:a"],
        )
        store.save(
            source_path="/a.db",
            target_path="/b.db",
            forward_sql="x",
            reverse_sql="y",
            objects_touched=["table:a"],
        )

        report = detect_conflicts(store)

        # Three migrations touching same object: (1,2), (1,3), (2,3)
        assert len(report.conflicts) == 3

    def test_no_conflicts_empty_store(self, tmp_path: Path) -> None:
        store = MigrationStore(base_dir=tmp_path / "store")
        report = detect_conflicts(store)
        assert not report.has_conflicts


# ---------------------------------------------------------------------------
# CLI migrate subcommand tests
# ---------------------------------------------------------------------------


class TestMigrateCLI:
    """Tests for the `patchworks migrate` CLI subcommands."""

    @staticmethod
    def _sa(tmp_path: Path) -> list[str]:
        """Return --store-dir pointing at an isolated temp directory."""
        return ["--store-dir", str(tmp_path / "cli_store")]

    def test_migrate_no_subcommand(self) -> None:
        result = _run("migrate")
        assert result.returncode == 1
        assert "specify" in result.stderr.lower()

    def test_generate_human(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )

        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "generate",
            str(left),
            str(right),
            "--name",
            "test-gen",
        )

        assert result.returncode == 0
        assert "generated" in result.stdout.lower() or "Migration" in result.stdout

    def test_generate_json(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )

        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )

        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert "forward_sql" in data
        assert "reverse_sql" in data
        assert data["dry_run"] is False

    def test_generate_dry_run(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'hello')"],
        )

        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "generate",
            str(left),
            str(right),
            "--dry-run",
            "--format",
            "json",
        )

        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert data["dry_run"] is True
        assert "migration" not in data

    def test_generate_missing_db(self, tmp_path: Path) -> None:
        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "generate",
            str(tmp_path / "nope.db"),
            str(tmp_path / "also_nope.db"),
        )
        assert result.returncode == 1

    def test_list_empty(self, tmp_path: Path) -> None:
        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "list",
            "--format",
            "json",
        )
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert isinstance(data, list)
        assert len(data) == 0

    def test_list_human(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        sa = self._sa(tmp_path)
        _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--name",
            "list-test",
        )

        result = _run("migrate", *sa, "list")

        assert result.returncode == 0
        assert "list-test" in result.stdout

    def test_conflicts_no_conflicts(self, tmp_path: Path) -> None:
        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "conflicts",
            "--format",
            "json",
        )
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert data["has_conflicts"] is False

    def test_show_missing(self, tmp_path: Path) -> None:
        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "show",
            "nonexistent-id",
        )
        assert result.returncode == 1

    def test_delete_missing(self, tmp_path: Path) -> None:
        result = _run(
            "migrate",
            *self._sa(tmp_path),
            "delete",
            "nonexistent-id",
        )
        assert result.returncode == 1

    def test_delete_json(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        sa = self._sa(tmp_path)

        gen_result = _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )
        mig_id = json.loads(gen_result.stdout)["migration"]["id"]

        del_result = _run(
            "migrate",
            *sa,
            "delete",
            mig_id,
            "--format",
            "json",
        )
        assert del_result.returncode == 0
        data = json.loads(del_result.stdout)
        assert data["deleted"] is True

    def test_show_json(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        sa = self._sa(tmp_path)

        gen_result = _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )
        mig_id = json.loads(gen_result.stdout)["migration"]["id"]

        show_result = _run(
            "migrate",
            *sa,
            "show",
            mig_id,
            "--format",
            "json",
        )
        assert show_result.returncode == 0
        data = json.loads(show_result.stdout)
        assert data["id"] == mig_id
        assert "forward_sql" in data

    def test_validate_json(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        sa = self._sa(tmp_path)

        gen_result = _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )
        mig_id = json.loads(gen_result.stdout)["migration"]["id"]

        val_result = _run(
            "migrate",
            *sa,
            "validate",
            mig_id,
            "--format",
            "json",
        )
        assert val_result.returncode == 0
        data = json.loads(val_result.stdout)
        assert data["valid"] is True

    def test_apply_json(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        target = _create_db(tmp_path / "target.db", [_TABLE])
        sa = self._sa(tmp_path)

        gen_result = _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )
        mig_id = json.loads(gen_result.stdout)["migration"]["id"]

        apply_result = _run(
            "migrate",
            *sa,
            "apply",
            mig_id,
            str(target),
            "--format",
            "json",
        )
        assert apply_result.returncode == 0
        data = json.loads(apply_result.stdout)
        assert data["success"] is True

    def test_apply_dry_run_cli(self, tmp_path: Path) -> None:
        left = _create_db(tmp_path / "left.db", [_TABLE])
        right = _create_db(
            tmp_path / "right.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        target = _create_db(tmp_path / "target.db", [_TABLE])
        sa = self._sa(tmp_path)

        gen_result = _run(
            "migrate",
            *sa,
            "generate",
            str(left),
            str(right),
            "--format",
            "json",
        )
        mig_id = json.loads(gen_result.stdout)["migration"]["id"]

        apply_result = _run(
            "migrate",
            *sa,
            "apply",
            mig_id,
            str(target),
            "--dry-run",
            "--format",
            "json",
        )
        assert apply_result.returncode == 0
        data = json.loads(apply_result.stdout)
        assert data["dry_run"] is True

        # Target should be unchanged.
        conn = sqlite3.connect(str(target))
        count = conn.execute("SELECT COUNT(*) FROM t").fetchone()[0]
        conn.close()
        assert count == 0


# ---------------------------------------------------------------------------
# End-to-end workflow tests
# ---------------------------------------------------------------------------


class TestEndToEnd:
    """Full workflow: generate → validate → apply → rollback."""

    def test_full_workflow(self, tmp_path: Path) -> None:
        """Generate, validate, apply, then rollback a migration."""
        v1 = _create_db(
            tmp_path / "v1.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'original')"],
        )
        v2 = _create_db(
            tmp_path / "v2.db",
            [
                _TABLE,
                "INSERT INTO t VALUES (1, 'original')",
                "INSERT INTO t VALUES (2, 'new_row')",
            ],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        # Generate.
        gen = generate_migration(v1, v2, store, name="add-row-2")
        assert gen.migration is not None

        # Validate.
        vr = validate_migration(gen.migration)
        assert vr.valid is True

        # Apply to a copy of v1.
        target = tmp_path / "target.db"
        _create_db(target, [_TABLE, "INSERT INTO t VALUES (1, 'original')"])

        ar = apply_migration(gen.migration, target, store)
        assert ar.success is True

        # Verify row was added.
        conn = sqlite3.connect(str(target))
        count = conn.execute("SELECT COUNT(*) FROM t").fetchone()[0]
        conn.close()
        assert count == 2

        # Rollback.
        rr = apply_migration(gen.migration, target, store, rollback=True)
        assert rr.success is True

        # Verify rollback: row should be removed.
        conn = sqlite3.connect(str(target))
        count = conn.execute("SELECT COUNT(*) FROM t").fetchone()[0]
        conn.close()
        assert count == 1

    def test_generate_validate_schema_change(self, tmp_path: Path) -> None:
        """Generate and validate a migration with schema changes."""
        v1 = _create_db(
            tmp_path / "v1.db",
            ["CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)"],
        )
        v2 = _create_db(
            tmp_path / "v2.db",
            [
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            ],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        gen = generate_migration(v1, v2, store, name="add-email-col")
        assert gen.migration is not None

        vr = validate_migration(gen.migration)
        assert vr.valid is True

    def test_squash_and_apply(self, tmp_path: Path) -> None:
        """Squash two migrations then apply the result."""
        v1 = _create_db(tmp_path / "v1.db", [_TABLE])
        v2 = _create_db(
            tmp_path / "v2.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        v3 = _create_db(
            tmp_path / "v3.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')", "INSERT INTO t VALUES (2, 'b')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")

        g1 = generate_migration(v1, v2, store, name="m1")
        g2 = generate_migration(v2, v3, store, name="m2")
        assert g1.migration is not None
        assert g2.migration is not None

        sq = squash_migrations(
            [g1.migration, g2.migration],
            store,
            name="all",
        )
        assert sq.migration is not None

        # Apply the squashed migration to a fresh v1.
        target = tmp_path / "target.db"
        _create_db(target, [_TABLE])

        ar = apply_migration(sq.migration, target, store)
        assert ar.success is True

        conn = sqlite3.connect(str(target))
        count = conn.execute("SELECT COUNT(*) FROM t").fetchone()[0]
        conn.close()
        assert count == 2

    def test_dry_run_never_modifies_target(self, tmp_path: Path) -> None:
        """Dry-run apply never touches the target database."""
        v1 = _create_db(tmp_path / "v1.db", [_TABLE])
        v2 = _create_db(
            tmp_path / "v2.db",
            [_TABLE, "INSERT INTO t VALUES (1, 'a')"],
        )
        store = MigrationStore(base_dir=tmp_path / "store")
        gen = generate_migration(v1, v2, store)
        assert gen.migration is not None

        target = tmp_path / "target.db"
        _create_db(target, [_TABLE])

        apply_migration(gen.migration, target, store, dry_run=True)

        # File should not have grown (no new rows).
        conn = sqlite3.connect(str(target))
        count = conn.execute("SELECT COUNT(*) FROM t").fetchone()[0]
        conn.close()
        assert count == 0
