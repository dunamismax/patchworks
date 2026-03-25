"""Streaming row-level diffing between two SQLite tables.

Uses primary-key matching with sorted iteration so that rows are never
fully materialized in memory.  Falls back to ``rowid`` when primary keys
diverge between the two databases, emitting a warning.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from patchworks.db.inspector import for_each_row
from patchworks.db.types import (
    CellChange,
    RowDiff,
    TableDataDiff,
    TableInfo,
)

if TYPE_CHECKING:
    import sqlite3
    from collections.abc import Iterator


def diff_table_data(
    left_conn: sqlite3.Connection,
    right_conn: sqlite3.Connection,
    left_table: TableInfo,
    right_table: TableInfo,
    *,
    page_size: int = 1000,
) -> TableDataDiff:
    """Compute row-level diffs for a table present in both databases.

    Streams rows from both sides using :func:`for_each_row` and merges
    them via a sorted merge-join on the key columns.

    If the primary-key columns diverge between the two table definitions,
    we fall back to ``rowid`` ordering (only valid for tables that have a
    ``rowid``) and emit a warning.
    """
    key_columns, warnings = _resolve_key_columns(left_table, right_table)

    left_iter = _keyed_rows(
        left_conn,
        left_table,
        key_columns,
        page_size=page_size,
    )
    right_iter = _keyed_rows(
        right_conn,
        right_table,
        key_columns,
        page_size=page_size,
    )

    row_diffs: list[RowDiff] = []
    added = 0
    removed = 0
    modified = 0

    left_row = _next_or_none(left_iter)
    right_row = _next_or_none(right_iter)

    while left_row is not None or right_row is not None:
        if left_row is not None and right_row is not None:
            l_key = left_row[0]
            r_key = right_row[0]

            if l_key == r_key:
                # Same key — check for modifications.
                l_vals = left_row[1]
                r_vals = right_row[1]
                changes = _cell_changes(l_vals, r_vals)
                if changes:
                    modified += 1
                    row_diffs.append(
                        RowDiff(
                            kind="modified",
                            key=l_key,
                            old_values=l_vals,
                            new_values=r_vals,
                            cell_changes=changes,
                        )
                    )
                left_row = _next_or_none(left_iter)
                right_row = _next_or_none(right_iter)

            elif l_key < r_key:
                # Left key absent from right → removed.
                removed += 1
                row_diffs.append(
                    RowDiff(
                        kind="removed",
                        key=l_key,
                        old_values=left_row[1],
                    )
                )
                left_row = _next_or_none(left_iter)

            else:
                # Right key absent from left → added.
                added += 1
                row_diffs.append(
                    RowDiff(
                        kind="added",
                        key=r_key,
                        new_values=right_row[1],
                    )
                )
                right_row = _next_or_none(right_iter)

        elif left_row is not None:
            removed += 1
            row_diffs.append(
                RowDiff(
                    kind="removed",
                    key=left_row[0],
                    old_values=left_row[1],
                )
            )
            left_row = _next_or_none(left_iter)

        else:
            assert right_row is not None
            added += 1
            row_diffs.append(
                RowDiff(
                    kind="added",
                    key=right_row[0],
                    new_values=right_row[1],
                )
            )
            right_row = _next_or_none(right_iter)

    return TableDataDiff(
        table_name=left_table.name,
        key_columns=key_columns,
        rows_added=added,
        rows_removed=removed,
        rows_modified=modified,
        row_diffs=tuple(row_diffs),
        warnings=tuple(warnings),
    )


# ---------------------------------------------------------------------------
# Key resolution
# ---------------------------------------------------------------------------


def _resolve_key_columns(
    left_table: TableInfo,
    right_table: TableInfo,
) -> tuple[tuple[str, ...], list[str]]:
    """Determine which columns to use as the merge key.

    Prefers the primary key when both sides agree.  Falls back to
    ``("rowid",)`` if the PKs diverge, with a warning.
    """
    warnings: list[str] = []

    if (
        left_table.primary_key_columns
        and left_table.primary_key_columns == right_table.primary_key_columns
    ):
        return left_table.primary_key_columns, warnings

    # Divergent or absent PKs — fall back to rowid.
    if left_table.without_rowid or right_table.without_rowid:
        # WITHOUT ROWID tables must have a PK.  If PKs disagree we
        # still try the left PK, but warn loudly.
        warnings.append(
            f"table {left_table.name!r}: primary keys differ between databases "
            f"(left={left_table.primary_key_columns!r}, "
            f"right={right_table.primary_key_columns!r}); "
            "using left PK — results may be unreliable"
        )
        key = left_table.primary_key_columns or right_table.primary_key_columns
        return key, warnings

    if not left_table.primary_key_columns and not right_table.primary_key_columns:
        warnings.append(
            f"table {left_table.name!r}: no primary key on either side; "
            "falling back to rowid ordering"
        )
        return ("rowid",), warnings

    warnings.append(
        f"table {left_table.name!r}: primary keys differ between databases "
        f"(left={left_table.primary_key_columns!r}, "
        f"right={right_table.primary_key_columns!r}); "
        "falling back to rowid ordering"
    )
    return ("rowid",), warnings


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# A keyed row: (key_tuple, full_row_dict)
type _KeyedRow = tuple[tuple[Any, ...], dict[str, Any]]


def _keyed_rows(
    conn: sqlite3.Connection,
    table: TableInfo,
    key_columns: tuple[str, ...],
    *,
    page_size: int = 1000,
) -> Iterator[_KeyedRow]:
    """Yield ``(key, row_dict)`` tuples from *table*, ordered by key.

    When *key_columns* is ``("rowid",)`` and the table has no explicit
    ``rowid`` column, we need the inspector's rowid-based ordering and
    inject the rowid into the key.
    """
    use_rowid = key_columns == ("rowid",)

    pk_cols: tuple[str, ...] = ()
    if not use_rowid:
        pk_cols = key_columns

    for row in for_each_row(
        conn,
        table.name,
        page_size=page_size,
        pk_columns=pk_cols,
        without_rowid=table.without_rowid,
    ):
        # When falling back to rowid, use the full row values as the
        # key since rowid is not in the row dict.
        key = tuple(row.values()) if use_rowid else tuple(row[c] for c in key_columns)
        yield key, row


def _next_or_none(it: Iterator[_KeyedRow]) -> _KeyedRow | None:
    """Advance *it* and return ``None`` at exhaustion."""
    return next(it, None)


def _cell_changes(old: dict[str, Any], new: dict[str, Any]) -> tuple[CellChange, ...]:
    """Compare two row dicts and return per-cell changes."""
    changes: list[CellChange] = []
    all_keys = dict.fromkeys([*old.keys(), *new.keys()])
    for col in all_keys:
        old_val = old.get(col)
        new_val = new.get(col)
        if old_val != new_val:
            changes.append(CellChange(column=col, old_value=old_val, new_value=new_val))
    return tuple(changes)
