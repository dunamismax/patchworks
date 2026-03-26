"""Migration generation, validation, rollback, and squashing.

Builds on the diff+export engine to produce forward and reverse SQL
migrations, validate them against a temporary copy, squash sequential
migrations, and detect conflicts between migrations.
"""

from __future__ import annotations

import shutil
import sqlite3
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from patchworks.db.differ import diff_databases
from patchworks.diff.export import export_as_sql

if TYPE_CHECKING:
    from patchworks.db.migration import MigrationInfo, MigrationStore


# ---------------------------------------------------------------------------
# Result types
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class GenerateResult:
    """Result of generating a migration."""

    migration: MigrationInfo | None
    """The stored migration, or ``None`` if ``dry_run=True``."""
    forward_sql: str
    reverse_sql: str
    objects_touched: list[str]
    dry_run: bool


@dataclass(frozen=True)
class ValidationResult:
    """Result of validating a migration."""

    migration_id: str
    valid: bool
    message: str
    remaining_differences: int = 0


@dataclass(frozen=True)
class ApplyResult:
    """Result of applying or rolling back a migration."""

    migration_id: str
    success: bool
    message: str
    dry_run: bool
    sql: str


@dataclass(frozen=True)
class SquashResult:
    """Result of squashing migrations."""

    migration: MigrationInfo | None
    """The new squashed migration, or ``None`` if ``dry_run=True``."""
    squashed_count: int
    forward_sql: str
    reverse_sql: str
    dry_run: bool


@dataclass(frozen=True)
class MigrationConflict:
    """A conflict between two migrations."""

    migration_a_id: str
    migration_b_id: str
    shared_objects: list[str]
    description: str


@dataclass(frozen=True)
class ConflictReport:
    """Report of all conflicts among stored migrations."""

    conflicts: list[MigrationConflict] = field(default_factory=list)

    @property
    def has_conflicts(self) -> bool:
        return len(self.conflicts) > 0


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _objects_touched_from_diff(
    left_path: str | Path, right_path: str | Path
) -> list[str]:
    """Extract names of database objects touched by a diff."""
    diff = diff_databases(left_path, right_path, data=False)
    sd = diff.schema_diff
    names: set[str] = set()

    for t in sd.tables_added:
        names.add(f"table:{t.name}")
    for t in sd.tables_removed:
        names.add(f"table:{t.name}")
    for tm in sd.tables_modified:
        names.add(f"table:{tm.table_name}")
    for i in sd.indexes_added:
        names.add(f"index:{i.name}")
    for i in sd.indexes_removed:
        names.add(f"index:{i.name}")
    for im in sd.indexes_modified:
        names.add(f"index:{im.name}")
    for tr in sd.triggers_added:
        names.add(f"trigger:{tr.name}")
    for tr in sd.triggers_removed:
        names.add(f"trigger:{tr.name}")
    for trm in sd.triggers_modified:
        names.add(f"trigger:{trm.name}")
    for v in sd.views_added:
        names.add(f"view:{v.name}")
    for v in sd.views_removed:
        names.add(f"view:{v.name}")
    for vm in sd.views_modified:
        names.add(f"view:{vm.name}")

    # Also include tables with row-level data changes.
    diff_full = diff_databases(left_path, right_path, data=True)
    for td in diff_full.table_data_diffs:
        if td.rows_added or td.rows_removed or td.rows_modified:
            names.add(f"table:{td.table_name}")

    return sorted(names)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def generate_migration(
    left_path: str | Path,
    right_path: str | Path,
    store: MigrationStore,
    *,
    name: str | None = None,
    dry_run: bool = False,
) -> GenerateResult:
    """Generate a forward+reverse migration from *left_path* to *right_path*.

    When ``dry_run=True``, SQL is generated but nothing is persisted.
    """
    left = str(Path(left_path).resolve())
    right = str(Path(right_path).resolve())

    # Forward: left -> right
    forward_diff = diff_databases(left, right)
    forward_sql = export_as_sql(forward_diff, right_path=right)

    # Reverse: right -> left
    reverse_diff = diff_databases(right, left)
    reverse_sql = export_as_sql(reverse_diff, right_path=left)

    objects = _objects_touched_from_diff(left, right)

    migration: MigrationInfo | None = None
    if not dry_run:
        migration = store.save(
            source_path=left,
            target_path=right,
            forward_sql=forward_sql,
            reverse_sql=reverse_sql,
            objects_touched=objects,
            name=name,
        )

    return GenerateResult(
        migration=migration,
        forward_sql=forward_sql,
        reverse_sql=reverse_sql,
        objects_touched=objects,
        dry_run=dry_run,
    )


def validate_migration(
    migration: MigrationInfo,
) -> ValidationResult:
    """Validate a migration by applying it to a temp copy and diffing.

    Creates a temporary copy of the source database, applies the forward
    SQL, then diffs the result against the target.  A valid migration
    produces zero remaining differences.
    """
    source = Path(migration.source_path)
    target = Path(migration.target_path)

    if not source.exists():
        return ValidationResult(
            migration_id=migration.id,
            valid=False,
            message=f"source database not found: {source}",
        )
    if not target.exists():
        return ValidationResult(
            migration_id=migration.id,
            valid=False,
            message=f"target database not found: {target}",
        )

    with tempfile.TemporaryDirectory() as tmp_dir:
        tmp_db = Path(tmp_dir) / "validation.db"
        shutil.copy2(str(source), str(tmp_db))

        # Apply forward SQL to the temporary copy.
        try:
            conn = sqlite3.connect(str(tmp_db))
            try:
                conn.executescript(migration.forward_sql)
            finally:
                conn.close()
        except sqlite3.Error as exc:
            return ValidationResult(
                migration_id=migration.id,
                valid=False,
                message=f"forward SQL failed: {exc}",
            )

        # Diff the result against the target.
        diff = diff_databases(str(tmp_db), str(target))
        if diff.has_changes:
            # Count remaining differences.
            remaining = 0
            if diff.schema_diff.has_changes:
                remaining += (
                    len(diff.schema_diff.tables_added)
                    + len(diff.schema_diff.tables_removed)
                    + len(diff.schema_diff.tables_modified)
                )
            for td in diff.table_data_diffs:
                remaining += td.rows_added + td.rows_removed + td.rows_modified
            return ValidationResult(
                migration_id=migration.id,
                valid=False,
                message=f"migration leaves {remaining} remaining differences",
                remaining_differences=remaining,
            )

    return ValidationResult(
        migration_id=migration.id,
        valid=True,
        message="migration is valid",
    )


def apply_migration(
    migration: MigrationInfo,
    target_db: str | Path,
    store: MigrationStore,
    *,
    dry_run: bool = False,
    rollback: bool = False,
) -> ApplyResult:
    """Apply (or rollback) a migration to *target_db*.

    When ``rollback=True``, applies the reverse SQL instead.
    When ``dry_run=True``, returns the SQL without modifying anything.
    """
    sql = migration.reverse_sql if rollback else migration.forward_sql
    action = "rollback" if rollback else "apply"

    if dry_run:
        return ApplyResult(
            migration_id=migration.id,
            success=True,
            message=f"dry run: {action} would execute {len(sql)} chars of SQL",
            dry_run=True,
            sql=sql,
        )

    target = Path(target_db)
    if not target.exists():
        return ApplyResult(
            migration_id=migration.id,
            success=False,
            message=f"target database not found: {target}",
            dry_run=False,
            sql=sql,
        )

    try:
        conn = sqlite3.connect(str(target))
        try:
            conn.executescript(sql)
        finally:
            conn.close()
    except sqlite3.Error as exc:
        return ApplyResult(
            migration_id=migration.id,
            success=False,
            message=f"{action} failed: {exc}",
            dry_run=False,
            sql=sql,
        )

    # Update applied state.
    if rollback:
        store.mark_unapplied(migration.id)
    else:
        store.mark_applied(migration.id)

    return ApplyResult(
        migration_id=migration.id,
        success=True,
        message=f"{action} succeeded",
        dry_run=False,
        sql=sql,
    )


def squash_migrations(
    migrations: list[MigrationInfo],
    store: MigrationStore,
    *,
    source_path: str | Path | None = None,
    name: str | None = None,
    dry_run: bool = False,
) -> SquashResult:
    """Squash a list of sequential migrations into a single migration.

    The squashed forward SQL is the concatenation of all forward SQL in
    sequence order.  The squashed reverse SQL is the concatenation of all
    reverse SQL in reverse sequence order.

    When ``dry_run=True``, returns the result without modifying the store.
    """
    if len(migrations) < 2:
        msg = "need at least 2 migrations to squash"
        raise ValueError(msg)

    # Sort by sequence.
    sorted_migs = sorted(migrations, key=lambda m: m.sequence)

    # Verify they are sequential (no gaps).
    seqs = [m.sequence for m in sorted_migs]
    for i in range(1, len(seqs)):
        if seqs[i] != seqs[i - 1] + 1:
            msg = (
                f"migrations are not sequential: gap between "
                f"sequence {seqs[i - 1]} and {seqs[i]}"
            )
            raise ValueError(msg)

    # Build combined SQL.
    forward_parts: list[str] = []
    reverse_parts: list[str] = []

    for m in sorted_migs:
        forward_parts.append(m.forward_sql)
    for m in reversed(sorted_migs):
        reverse_parts.append(m.reverse_sql)

    forward_sql = "\n".join(forward_parts)
    reverse_sql = "\n".join(reverse_parts)

    # Combine objects touched.
    all_objects: set[str] = set()
    for m in sorted_migs:
        if m.objects_touched:
            all_objects.update(m.objects_touched.split(","))
    all_objects.discard("")
    objects_list = sorted(all_objects)

    src = str(source_path) if source_path else sorted_migs[0].source_path

    if dry_run:
        return SquashResult(
            migration=None,
            squashed_count=len(sorted_migs),
            forward_sql=forward_sql,
            reverse_sql=reverse_sql,
            dry_run=True,
        )

    old_ids = [m.id for m in sorted_migs]
    squashed = store.replace_with_squashed(
        old_ids,
        source_path=src,
        target_path=sorted_migs[-1].target_path,
        forward_sql=forward_sql,
        reverse_sql=reverse_sql,
        objects_touched=objects_list,
        name=name,
    )

    return SquashResult(
        migration=squashed,
        squashed_count=len(sorted_migs),
        forward_sql=forward_sql,
        reverse_sql=reverse_sql,
        dry_run=False,
    )


def detect_conflicts(store: MigrationStore) -> ConflictReport:
    """Detect conflicts between unapplied migrations.

    Two migrations conflict when they touch the same database objects.
    """
    migrations = [m for m in store.list() if not m.applied]
    conflicts: list[MigrationConflict] = []

    for i, a in enumerate(migrations):
        a_objects = set(a.objects_touched.split(",")) if a.objects_touched else set()
        a_objects.discard("")
        for b in migrations[i + 1 :]:
            b_objects = (
                set(b.objects_touched.split(",")) if b.objects_touched else set()
            )
            b_objects.discard("")
            shared = sorted(a_objects & b_objects)
            if shared:
                conflicts.append(
                    MigrationConflict(
                        migration_a_id=a.id,
                        migration_b_id=b.id,
                        shared_objects=shared,
                        description=(
                            f"migrations {a.id[:8]} (seq {a.sequence}) and "
                            f"{b.id[:8]} (seq {b.sequence}) both touch: "
                            f"{', '.join(shared)}"
                        ),
                    )
                )

    return ConflictReport(conflicts=conflicts)
