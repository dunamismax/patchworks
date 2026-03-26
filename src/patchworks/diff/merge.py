"""Three-way merge engine for SQLite databases.

Diffs both *left* and *right* databases against a common *ancestor*,
merges non-conflicting changes, and surfaces conflicts with enough
context for manual resolution.

Conflict types:
- **row conflict** — the same row was modified differently on both sides.
- **schema conflict** — the same table's schema was modified differently.
- **delete-modify conflict** — one side deleted a row the other modified.
- **table-delete conflict** — one side dropped a table the other modified.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, Literal

from patchworks.db.differ import diff_databases

if TYPE_CHECKING:
    from patchworks.db.types import (
        DatabaseDiff,
        RowDiff,
        SchemaDiff,
        TableDataDiff,
    )

# ---------------------------------------------------------------------------
# Merge result types
# ---------------------------------------------------------------------------

ConflictKind = Literal[
    "row",
    "schema",
    "delete-modify",
    "table-delete",
]


@dataclass(frozen=True)
class MergeConflict:
    """A single merge conflict."""

    kind: ConflictKind
    table: str
    description: str
    left_detail: str = ""
    right_detail: str = ""
    key: tuple[Any, ...] = ()
    """Row key for row-level conflicts; empty for schema-level."""


@dataclass(frozen=True)
class MergedRow:
    """A row change that was cleanly merged from one side."""

    table: str
    kind: str
    """``"added"``, ``"removed"``, or ``"modified"``."""
    source: str
    """``"left"`` or ``"right"``."""
    key: tuple[Any, ...]
    values: dict[str, Any] | None = None


@dataclass(frozen=True)
class MergedSchemaChange:
    """A schema change that was cleanly merged from one side."""

    table: str
    kind: str
    """``"added"``, ``"removed"``, or ``"modified"``."""
    source: str
    """``"left"`` or ``"right"``."""
    sql: str | None = None


@dataclass(frozen=True)
class MergeResult:
    """Complete result of a three-way merge."""

    ancestor_path: str
    left_path: str
    right_path: str
    conflicts: tuple[MergeConflict, ...] = field(default_factory=tuple)
    merged_rows: tuple[MergedRow, ...] = field(default_factory=tuple)
    merged_schema: tuple[MergedSchemaChange, ...] = field(default_factory=tuple)

    @property
    def has_conflicts(self) -> bool:
        """``True`` if any merge conflicts were detected."""
        return len(self.conflicts) > 0

    @property
    def is_clean(self) -> bool:
        """``True`` if the merge is fully clean (no conflicts)."""
        return not self.has_conflicts


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def merge_databases(
    ancestor_path: str,
    left_path: str,
    right_path: str,
    *,
    page_size: int = 1000,
) -> MergeResult:
    """Perform a three-way merge of two databases against a common ancestor.

    Diffs *left* and *right* against *ancestor*, merges non-conflicting
    changes, and reports conflicts.
    """
    left_diff = diff_databases(ancestor_path, left_path, page_size=page_size)
    right_diff = diff_databases(ancestor_path, right_path, page_size=page_size)

    conflicts: list[MergeConflict] = []
    merged_rows: list[MergedRow] = []
    merged_schema: list[MergedSchemaChange] = []

    # --- Schema-level merge -----------------------------------------------
    _merge_schema(
        left_diff.schema_diff,
        right_diff.schema_diff,
        conflicts,
        merged_schema,
    )

    # --- Row-level merge --------------------------------------------------
    _merge_rows(
        left_diff,
        right_diff,
        conflicts,
        merged_rows,
    )

    return MergeResult(
        ancestor_path=ancestor_path,
        left_path=left_path,
        right_path=right_path,
        conflicts=tuple(conflicts),
        merged_rows=tuple(merged_rows),
        merged_schema=tuple(merged_schema),
    )


# ---------------------------------------------------------------------------
# Schema merging
# ---------------------------------------------------------------------------


def _merge_schema(
    left_sd: SchemaDiff,
    right_sd: SchemaDiff,
    conflicts: list[MergeConflict],
    merged: list[MergedSchemaChange],
) -> None:
    """Merge schema-level changes, detecting conflicts."""
    # Table-level changes.
    left_added = {t.name for t in left_sd.tables_added}
    right_added = {t.name for t in right_sd.tables_added}
    left_removed = {t.name for t in left_sd.tables_removed}
    right_removed = {t.name for t in right_sd.tables_removed}
    left_modified = {tm.table_name: tm for tm in left_sd.tables_modified}
    right_modified = {tm.table_name: tm for tm in right_sd.tables_modified}

    # Both added the same table — conflict if SQL differs, clean merge if identical.
    for name in left_added & right_added:
        lt = next(t for t in left_sd.tables_added if t.name == name)
        rt = next(t for t in right_sd.tables_added if t.name == name)
        if lt.sql == rt.sql:
            merged.append(
                MergedSchemaChange(table=name, kind="added", source="both", sql=lt.sql)
            )
        else:
            conflicts.append(
                MergeConflict(
                    kind="schema",
                    table=name,
                    description=(
                        f"table {name!r} added on both sides with different schemas"
                    ),
                    left_detail=lt.sql or "",
                    right_detail=rt.sql or "",
                )
            )

    # Added only on one side — clean merge.
    for name in left_added - right_added:
        lt = next(t for t in left_sd.tables_added if t.name == name)
        # Check for table-delete conflict: right removed a table that left added?
        # This wouldn't happen since the table didn't exist in ancestor.
        merged.append(
            MergedSchemaChange(table=name, kind="added", source="left", sql=lt.sql)
        )
    for name in right_added - left_added:
        rt = next(t for t in right_sd.tables_added if t.name == name)
        merged.append(
            MergedSchemaChange(table=name, kind="added", source="right", sql=rt.sql)
        )

    # Both removed the same table — clean.
    for name in left_removed & right_removed:
        merged.append(MergedSchemaChange(table=name, kind="removed", source="both"))

    # Removed only on one side — check for table-delete conflict.
    for name in left_removed - right_removed:
        if name in right_modified:
            conflicts.append(
                MergeConflict(
                    kind="table-delete",
                    table=name,
                    description=f"table {name!r} dropped on left but modified on right",
                    right_detail=right_modified[name].new_sql or "",
                )
            )
        else:
            merged.append(MergedSchemaChange(table=name, kind="removed", source="left"))
    for name in right_removed - left_removed:
        if name in left_modified:
            conflicts.append(
                MergeConflict(
                    kind="table-delete",
                    table=name,
                    description=f"table {name!r} dropped on right but modified on left",
                    left_detail=left_modified[name].new_sql or "",
                )
            )
        else:
            merged.append(
                MergedSchemaChange(table=name, kind="removed", source="right")
            )

    # Both modified the same table — conflict if differently, clean if same.
    for name in set(left_modified) & set(right_modified):
        lm = left_modified[name]
        rm = right_modified[name]
        if lm.new_sql == rm.new_sql:
            merged.append(
                MergedSchemaChange(
                    table=name, kind="modified", source="both", sql=lm.new_sql
                )
            )
        else:
            conflicts.append(
                MergeConflict(
                    kind="schema",
                    table=name,
                    description=f"table {name!r} modified differently on both sides",
                    left_detail=lm.new_sql or "",
                    right_detail=rm.new_sql or "",
                )
            )

    # Modified on one side only — clean merge.
    for name in set(left_modified) - set(right_modified):
        if name not in right_removed:
            lm = left_modified[name]
            merged.append(
                MergedSchemaChange(
                    table=name, kind="modified", source="left", sql=lm.new_sql
                )
            )
    for name in set(right_modified) - set(left_modified):
        if name not in left_removed:
            rm = right_modified[name]
            merged.append(
                MergedSchemaChange(
                    table=name, kind="modified", source="right", sql=rm.new_sql
                )
            )


# ---------------------------------------------------------------------------
# Row-level merging
# ---------------------------------------------------------------------------


def _merge_rows(
    left_diff: DatabaseDiff,
    right_diff: DatabaseDiff,
    conflicts: list[MergeConflict],
    merged: list[MergedRow],
) -> None:
    """Merge row-level changes across both diffs."""
    left_by_table = {td.table_name: td for td in left_diff.table_data_diffs}
    right_by_table = {td.table_name: td for td in right_diff.table_data_diffs}

    all_tables = sorted(set(left_by_table) | set(right_by_table))

    for table in all_tables:
        left_td = left_by_table.get(table)
        right_td = right_by_table.get(table)

        if left_td and not right_td:
            # Only left has row changes — all clean.
            for rd in left_td.row_diffs:
                merged.append(
                    MergedRow(
                        table=table,
                        kind=rd.kind,
                        source="left",
                        key=rd.key,
                        values=rd.new_values or rd.old_values,
                    )
                )
        elif right_td and not left_td:
            # Only right has row changes — all clean.
            for rd in right_td.row_diffs:
                merged.append(
                    MergedRow(
                        table=table,
                        kind=rd.kind,
                        source="right",
                        key=rd.key,
                        values=rd.new_values or rd.old_values,
                    )
                )
        elif left_td and right_td:
            _merge_table_rows(table, left_td, right_td, conflicts, merged)


def _merge_table_rows(
    table: str,
    left_td: TableDataDiff,
    right_td: TableDataDiff,
    conflicts: list[MergeConflict],
    merged: list[MergedRow],
) -> None:
    """Merge row changes for a single table present on both sides."""
    left_by_key: dict[tuple[Any, ...], RowDiff] = {
        rd.key: rd for rd in left_td.row_diffs
    }
    right_by_key: dict[tuple[Any, ...], RowDiff] = {
        rd.key: rd for rd in right_td.row_diffs
    }

    all_keys = sorted(set(left_by_key) | set(right_by_key))

    for key in all_keys:
        l_rd = left_by_key.get(key)
        r_rd = right_by_key.get(key)

        if l_rd and not r_rd:
            # Only left touched this row.
            merged.append(
                MergedRow(
                    table=table,
                    kind=l_rd.kind,
                    source="left",
                    key=key,
                    values=l_rd.new_values or l_rd.old_values,
                )
            )
        elif r_rd and not l_rd:
            # Only right touched this row.
            merged.append(
                MergedRow(
                    table=table,
                    kind=r_rd.kind,
                    source="right",
                    key=key,
                    values=r_rd.new_values or r_rd.old_values,
                )
            )
        elif l_rd and r_rd:
            _merge_overlapping_row(table, key, l_rd, r_rd, conflicts, merged)


def _merge_overlapping_row(
    table: str,
    key: tuple[Any, ...],
    left: RowDiff,
    right: RowDiff,
    conflicts: list[MergeConflict],
    merged: list[MergedRow],
) -> None:
    """Handle a row touched by both sides."""
    # Both added — conflict if different, clean if identical.
    if left.kind == "added" and right.kind == "added":
        if left.new_values == right.new_values:
            merged.append(
                MergedRow(
                    table=table,
                    kind="added",
                    source="both",
                    key=key,
                    values=left.new_values,
                )
            )
        else:
            conflicts.append(
                MergeConflict(
                    kind="row",
                    table=table,
                    key=key,
                    description=(
                        f"row {_key_str(key)} added with different values on both sides"
                    ),
                    left_detail=_row_repr(left.new_values),
                    right_detail=_row_repr(right.new_values),
                )
            )
        return

    # Both removed — clean.
    if left.kind == "removed" and right.kind == "removed":
        merged.append(
            MergedRow(
                table=table,
                kind="removed",
                source="both",
                key=key,
                values=left.old_values,
            )
        )
        return

    # One deleted, other modified — delete-modify conflict.
    if left.kind == "removed" and right.kind == "modified":
        conflicts.append(
            MergeConflict(
                kind="delete-modify",
                table=table,
                key=key,
                description=(
                    f"row {_key_str(key)} deleted on left but modified on right"
                ),
                right_detail=_row_repr(right.new_values),
            )
        )
        return
    if right.kind == "removed" and left.kind == "modified":
        conflicts.append(
            MergeConflict(
                kind="delete-modify",
                table=table,
                key=key,
                description=(
                    f"row {_key_str(key)} modified on left but deleted on right"
                ),
                left_detail=_row_repr(left.new_values),
            )
        )
        return

    # Both modified — check if changes are compatible.
    if left.kind == "modified" and right.kind == "modified":
        _merge_modified_row(table, key, left, right, conflicts, merged)
        return

    # Any other combination (e.g. added+removed) — conflict.
    conflicts.append(
        MergeConflict(
            kind="row",
            table=table,
            key=key,
            description=(
                f"row {_key_str(key)} changed incompatibly: "
                f"left={left.kind}, right={right.kind}"
            ),
        )
    )


def _merge_modified_row(
    table: str,
    key: tuple[Any, ...],
    left: RowDiff,
    right: RowDiff,
    conflicts: list[MergeConflict],
    merged: list[MergedRow],
) -> None:
    """Merge two modifications to the same row.

    Non-conflicting when each side modifies different columns.
    Conflicting when the same column is changed to different values.
    """
    left_changes = {cc.column: cc for cc in left.cell_changes}
    right_changes = {cc.column: cc for cc in right.cell_changes}

    overlap = set(left_changes) & set(right_changes)

    # Check if overlapping columns were changed to the same value.
    real_conflicts: list[str] = []
    for col in overlap:
        if left_changes[col].new_value != right_changes[col].new_value:
            real_conflicts.append(col)

    if real_conflicts:
        left_vals = {c: left_changes[c].new_value for c in real_conflicts}
        right_vals = {c: right_changes[c].new_value for c in real_conflicts}
        conflicts.append(
            MergeConflict(
                kind="row",
                table=table,
                key=key,
                description=(
                    f"row {_key_str(key)} modified on both sides with "
                    "conflicting values in columns: "
                    f"{', '.join(sorted(real_conflicts))}"
                ),
                left_detail=repr(left_vals),
                right_detail=repr(right_vals),
            )
        )
        return

    # Non-conflicting: merge cell changes from both sides.
    # Start from the old values and apply both sets of changes.
    base = dict(left.old_values or {})
    for cc in left.cell_changes:
        base[cc.column] = cc.new_value
    for cc in right.cell_changes:
        base[cc.column] = cc.new_value

    merged.append(
        MergedRow(
            table=table,
            kind="modified",
            source="both",
            key=key,
            values=base,
        )
    )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _key_str(key: tuple[Any, ...]) -> str:
    """Format a row key for display."""
    return "[" + ", ".join(repr(v) for v in key) + "]"


def _row_repr(row: dict[str, Any] | None) -> str:
    """Format a row dict for display."""
    if row is None:
        return "<no values>"
    return repr(row)
