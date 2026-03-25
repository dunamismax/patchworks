"""Core data types for SQLite inspection and diffing.

All types are plain dataclasses — no external dependencies.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class ColumnInfo:
    """Metadata for a single column in a table or view."""

    name: str
    type: str
    """Declared type string (may be empty for typeless columns)."""
    notnull: bool
    default_value: str | None
    """Default value expression as a string, or ``None``."""
    primary_key: int
    """1-based position in the primary key, or 0 if not part of the PK."""


@dataclass(frozen=True)
class IndexInfo:
    """Metadata for an index."""

    name: str
    table_name: str
    unique: bool
    columns: tuple[str, ...]
    """Ordered column names in the index."""
    partial: bool
    """``True`` if the index has a ``WHERE`` clause."""
    sql: str | None
    """Original ``CREATE INDEX`` statement, or ``None`` for auto-indexes."""


@dataclass(frozen=True)
class TriggerInfo:
    """Metadata for a trigger."""

    name: str
    table_name: str
    sql: str
    """Original ``CREATE TRIGGER`` statement."""


@dataclass(frozen=True)
class ViewInfo:
    """Metadata for a view."""

    name: str
    columns: tuple[ColumnInfo, ...]
    sql: str
    """Original ``CREATE VIEW`` statement."""


@dataclass(frozen=True)
class TableInfo:
    """Metadata for a table."""

    name: str
    columns: tuple[ColumnInfo, ...]
    primary_key_columns: tuple[str, ...]
    """Column names forming the primary key, in PK order."""
    without_rowid: bool
    row_count: int
    indexes: tuple[IndexInfo, ...]
    triggers: tuple[TriggerInfo, ...]
    sql: str
    """Original ``CREATE TABLE`` statement."""


@dataclass(frozen=True)
class DatabaseSummary:
    """Complete summary of a SQLite database."""

    path: str
    page_size: int
    page_count: int
    journal_mode: str
    tables: tuple[TableInfo, ...] = field(default_factory=tuple)
    views: tuple[ViewInfo, ...] = field(default_factory=tuple)
    indexes: tuple[IndexInfo, ...] = field(default_factory=tuple)
    triggers: tuple[TriggerInfo, ...] = field(default_factory=tuple)


# ---------------------------------------------------------------------------
# Diff result types
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class CellChange:
    """A single cell-level change within a modified row."""

    column: str
    old_value: Any
    new_value: Any


@dataclass(frozen=True)
class RowDiff:
    """A single row-level difference.

    *kind* is one of ``"added"``, ``"removed"``, or ``"modified"``.

    For ``"added"`` rows, *new_values* holds the full row and *old_values* is
    ``None``.  For ``"removed"`` rows, *old_values* holds the full row and
    *new_values* is ``None``.  For ``"modified"`` rows, both are present and
    *cell_changes* details which columns differ.

    *key* holds the primary-key (or rowid fallback) values used to match the
    row across the two databases.
    """

    kind: str
    """``"added"``, ``"removed"``, or ``"modified"``."""
    key: tuple[Any, ...]
    old_values: dict[str, Any] | None = None
    new_values: dict[str, Any] | None = None
    cell_changes: tuple[CellChange, ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class TableDataDiff:
    """Row-level diff summary for a single table."""

    table_name: str
    key_columns: tuple[str, ...]
    """Columns used to match rows (PK or rowid fallback)."""
    rows_added: int = 0
    rows_removed: int = 0
    rows_modified: int = 0
    row_diffs: tuple[RowDiff, ...] = field(default_factory=tuple)
    warnings: tuple[str, ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class TableSchemaDiff:
    """Schema-level diff for a single table."""

    table_name: str
    old_sql: str | None
    """``CREATE TABLE`` in the left database, or ``None`` if the table is new."""
    new_sql: str | None
    """``CREATE TABLE`` in the right database, or ``None`` if the table was dropped."""
    columns_added: tuple[ColumnInfo, ...] = field(default_factory=tuple)
    columns_removed: tuple[ColumnInfo, ...] = field(default_factory=tuple)
    columns_modified: tuple[tuple[ColumnInfo, ColumnInfo], ...] = field(
        default_factory=tuple
    )
    """Pairs of ``(old_column, new_column)`` for columns whose properties changed."""


@dataclass(frozen=True)
class IndexSchemaDiff:
    """Schema-level diff for a single index."""

    name: str
    old_sql: str | None
    new_sql: str | None


@dataclass(frozen=True)
class TriggerSchemaDiff:
    """Schema-level diff for a single trigger."""

    name: str
    old_sql: str | None
    new_sql: str | None


@dataclass(frozen=True)
class ViewSchemaDiff:
    """Schema-level diff for a single view."""

    name: str
    old_sql: str | None
    new_sql: str | None


@dataclass(frozen=True)
class SchemaDiff:
    """Complete schema-level diff between two databases."""

    tables_added: tuple[TableInfo, ...] = field(default_factory=tuple)
    tables_removed: tuple[TableInfo, ...] = field(default_factory=tuple)
    tables_modified: tuple[TableSchemaDiff, ...] = field(default_factory=tuple)
    indexes_added: tuple[IndexInfo, ...] = field(default_factory=tuple)
    indexes_removed: tuple[IndexInfo, ...] = field(default_factory=tuple)
    indexes_modified: tuple[IndexSchemaDiff, ...] = field(default_factory=tuple)
    triggers_added: tuple[TriggerInfo, ...] = field(default_factory=tuple)
    triggers_removed: tuple[TriggerInfo, ...] = field(default_factory=tuple)
    triggers_modified: tuple[TriggerSchemaDiff, ...] = field(default_factory=tuple)
    views_added: tuple[ViewInfo, ...] = field(default_factory=tuple)
    views_removed: tuple[ViewInfo, ...] = field(default_factory=tuple)
    views_modified: tuple[ViewSchemaDiff, ...] = field(default_factory=tuple)

    @property
    def has_changes(self) -> bool:
        """``True`` if any schema-level difference was detected."""
        return bool(
            self.tables_added
            or self.tables_removed
            or self.tables_modified
            or self.indexes_added
            or self.indexes_removed
            or self.indexes_modified
            or self.triggers_added
            or self.triggers_removed
            or self.triggers_modified
            or self.views_added
            or self.views_removed
            or self.views_modified
        )


@dataclass(frozen=True)
class DatabaseDiff:
    """Complete diff result combining schema and row-level diffs."""

    left_path: str
    right_path: str
    schema_diff: SchemaDiff
    table_data_diffs: tuple[TableDataDiff, ...] = field(default_factory=tuple)
    warnings: tuple[str, ...] = field(default_factory=tuple)

    @property
    def has_changes(self) -> bool:
        """``True`` if any difference was detected."""
        if self.schema_diff.has_changes:
            return True
        return any(
            d.rows_added or d.rows_removed or d.rows_modified
            for d in self.table_data_diffs
        )
