"""Database inspection, snapshots, and diff orchestration."""

from __future__ import annotations

from patchworks.db.inspector import (
    for_each_row,
    inspect_database,
    inspect_table,
    read_rows,
)
from patchworks.db.snapshot import SnapshotInfo, SnapshotStore
from patchworks.db.types import (
    CellChange,
    ColumnInfo,
    DatabaseDiff,
    DatabaseSummary,
    IndexInfo,
    IndexSchemaDiff,
    RowDiff,
    SchemaDiff,
    TableDataDiff,
    TableInfo,
    TableSchemaDiff,
    TriggerInfo,
    TriggerSchemaDiff,
    ViewInfo,
    ViewSchemaDiff,
)

__all__ = [
    "CellChange",
    "ColumnInfo",
    "DatabaseDiff",
    "DatabaseSummary",
    "IndexInfo",
    "IndexSchemaDiff",
    "RowDiff",
    "SchemaDiff",
    "SnapshotInfo",
    "SnapshotStore",
    "TableDataDiff",
    "TableInfo",
    "TableSchemaDiff",
    "TriggerInfo",
    "TriggerSchemaDiff",
    "ViewInfo",
    "ViewSchemaDiff",
    "for_each_row",
    "inspect_database",
    "inspect_table",
    "read_rows",
]


def __getattr__(name: str) -> object:
    # Lazy import to break circular dependency: db.__init__ -> db.differ ->
    # diff.data -> db.inspector -> db.__init__.
    if name == "diff_databases":
        from patchworks.db.differ import diff_databases

        return diff_databases
    msg = f"module {__name__!r} has no attribute {name!r}"
    raise AttributeError(msg)
