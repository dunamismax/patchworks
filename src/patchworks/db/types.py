"""Core data types for SQLite inspection.

All types are plain dataclasses — no external dependencies.
"""

from __future__ import annotations

from dataclasses import dataclass, field


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
