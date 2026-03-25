"""Database inspection, snapshots, and diff orchestration."""

from patchworks.db.differ import diff_databases
from patchworks.db.inspector import (
    for_each_row,
    inspect_database,
    inspect_table,
    read_rows,
)
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
    "TableDataDiff",
    "TableInfo",
    "TableSchemaDiff",
    "TriggerInfo",
    "TriggerSchemaDiff",
    "ViewInfo",
    "ViewSchemaDiff",
    "diff_databases",
    "for_each_row",
    "inspect_database",
    "inspect_table",
    "read_rows",
]
