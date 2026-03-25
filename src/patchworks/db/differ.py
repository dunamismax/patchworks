"""High-level diff orchestration combining schema and row diffs.

This module is the public entry point for comparing two SQLite databases.
It ties together :mod:`patchworks.diff.schema` and
:mod:`patchworks.diff.data` and returns a single :class:`DatabaseDiff`.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from patchworks.db.inspector import _open_readonly, inspect_database

if TYPE_CHECKING:
    from pathlib import Path
from patchworks.db.types import (
    DatabaseDiff,
    SchemaDiff,
    TableDataDiff,
)
from patchworks.diff.data import diff_table_data
from patchworks.diff.schema import diff_schemas


def diff_databases(
    left_path: str | Path,
    right_path: str | Path,
    *,
    page_size: int = 1000,
    data: bool = True,
) -> DatabaseDiff:
    """Compare two SQLite databases at the schema and row level.

    Parameters
    ----------
    left_path:
        Path to the left (base) database.
    right_path:
        Path to the right (target) database.
    page_size:
        Number of rows fetched per internal page during streaming comparison.
    data:
        If ``False``, skip row-level diffing and only compare schemas.

    Returns
    -------
    DatabaseDiff
        A complete diff result with both schema and row-level details.
    """
    left_summary = inspect_database(left_path)
    right_summary = inspect_database(right_path)

    schema_diff: SchemaDiff = diff_schemas(left_summary, right_summary)

    if not data:
        return DatabaseDiff(
            left_path=left_summary.path,
            right_path=right_summary.path,
            schema_diff=schema_diff,
        )

    # Row-level diffs for tables present in both databases.
    left_tables = {t.name: t for t in left_summary.tables}
    right_tables = {t.name: t for t in right_summary.tables}
    common_tables = sorted(set(left_tables) & set(right_tables))

    # Exclude tables whose schemas changed in incompatible ways (column sets
    # differ) — comparing rows when the column layout changed is not
    # meaningful at this level.
    modified_table_names = {td.table_name for td in schema_diff.tables_modified}

    table_data_diffs: list[TableDataDiff] = []
    all_warnings: list[str] = []

    left_conn = _open_readonly(left_path)
    right_conn = _open_readonly(right_path)
    try:
        for table_name in common_tables:
            lt = left_tables[table_name]
            rt = right_tables[table_name]

            # If the column sets differ, skip row diffing for this table
            # and note a warning instead.
            left_col_names = tuple(c.name for c in lt.columns)
            right_col_names = tuple(c.name for c in rt.columns)

            if table_name in modified_table_names and left_col_names != right_col_names:
                all_warnings.append(
                    f"table {table_name!r}: column layout changed — "
                    "skipping row-level diff"
                )
                continue

            td = diff_table_data(
                left_conn,
                right_conn,
                lt,
                rt,
                page_size=page_size,
            )
            if td.warnings:
                all_warnings.extend(td.warnings)
            table_data_diffs.append(td)
    finally:
        left_conn.close()
        right_conn.close()

    return DatabaseDiff(
        left_path=left_summary.path,
        right_path=right_summary.path,
        schema_diff=schema_diff,
        table_data_diffs=tuple(table_data_diffs),
        warnings=tuple(all_warnings),
    )
