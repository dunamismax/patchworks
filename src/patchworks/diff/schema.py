"""Schema-level diffing between two SQLite databases.

Compares tables, indexes, triggers, and views by name, detecting
added, removed, and modified objects.
"""

from __future__ import annotations

from patchworks.db.types import (
    ColumnInfo,
    DatabaseSummary,
    IndexInfo,
    IndexSchemaDiff,
    SchemaDiff,
    TableInfo,
    TableSchemaDiff,
    TriggerInfo,
    TriggerSchemaDiff,
    ViewInfo,
    ViewSchemaDiff,
)


def diff_schemas(left: DatabaseSummary, right: DatabaseSummary) -> SchemaDiff:
    """Compare schemas of two database summaries.

    Returns a :class:`SchemaDiff` describing all object-level changes.
    """
    return SchemaDiff(
        tables_added=_added(left.tables, right.tables),
        tables_removed=_removed(left.tables, right.tables),
        tables_modified=_diff_tables(left.tables, right.tables),
        indexes_added=_added(left.indexes, right.indexes),
        indexes_removed=_removed(left.indexes, right.indexes),
        indexes_modified=_diff_indexes(left.indexes, right.indexes),
        triggers_added=_added(left.triggers, right.triggers),
        triggers_removed=_removed(left.triggers, right.triggers),
        triggers_modified=_diff_triggers(left.triggers, right.triggers),
        views_added=_added(left.views, right.views),
        views_removed=_removed(left.views, right.views),
        views_modified=_diff_views(left.views, right.views),
    )


# ---------------------------------------------------------------------------
# Generic set helpers
# ---------------------------------------------------------------------------

type _Named = TableInfo | IndexInfo | TriggerInfo | ViewInfo


def _by_name[T: _Named](items: tuple[T, ...]) -> dict[str, T]:
    """Build a name → object mapping."""
    return {item.name: item for item in items}


def _added[T: _Named](left: tuple[T, ...], right: tuple[T, ...]) -> tuple[T, ...]:
    """Return items present in *right* but not *left*."""
    left_names = {item.name for item in left}
    return tuple(item for item in right if item.name not in left_names)


def _removed[T: _Named](left: tuple[T, ...], right: tuple[T, ...]) -> tuple[T, ...]:
    """Return items present in *left* but not *right*."""
    right_names = {item.name for item in right}
    return tuple(item for item in left if item.name not in right_names)


# ---------------------------------------------------------------------------
# Table diffing
# ---------------------------------------------------------------------------


def _diff_tables(
    left: tuple[TableInfo, ...], right: tuple[TableInfo, ...]
) -> tuple[TableSchemaDiff, ...]:
    """Find tables present in both databases whose schemas differ."""
    left_map = _by_name(left)
    right_map = _by_name(right)
    common = sorted(set(left_map) & set(right_map))

    results: list[TableSchemaDiff] = []
    for name in common:
        lt = left_map[name]
        rt = right_map[name]
        if lt.sql == rt.sql:
            continue

        left_cols = {c.name: c for c in lt.columns}
        right_cols = {c.name: c for c in rt.columns}

        cols_added = tuple(c for c in rt.columns if c.name not in left_cols)
        cols_removed = tuple(c for c in lt.columns if c.name not in right_cols)
        cols_modified = _modified_columns(left_cols, right_cols)

        results.append(
            TableSchemaDiff(
                table_name=name,
                old_sql=lt.sql,
                new_sql=rt.sql,
                columns_added=cols_added,
                columns_removed=cols_removed,
                columns_modified=cols_modified,
            )
        )
    return tuple(results)


def _modified_columns(
    left_cols: dict[str, ColumnInfo],
    right_cols: dict[str, ColumnInfo],
) -> tuple[tuple[ColumnInfo, ColumnInfo], ...]:
    """Return ``(old, new)`` pairs for columns present in both but with
    differing properties (type, notnull, default, pk position)."""
    common = sorted(set(left_cols) & set(right_cols))
    pairs: list[tuple[ColumnInfo, ColumnInfo]] = []
    for name in common:
        lc = left_cols[name]
        rc = right_cols[name]
        if (lc.type, lc.notnull, lc.default_value, lc.primary_key) != (
            rc.type,
            rc.notnull,
            rc.default_value,
            rc.primary_key,
        ):
            pairs.append((lc, rc))
    return tuple(pairs)


# ---------------------------------------------------------------------------
# Index diffing
# ---------------------------------------------------------------------------


def _diff_indexes(
    left: tuple[IndexInfo, ...], right: tuple[IndexInfo, ...]
) -> tuple[IndexSchemaDiff, ...]:
    """Find indexes present in both databases whose SQL differs."""
    left_map = _by_name(left)
    right_map = _by_name(right)
    common = sorted(set(left_map) & set(right_map))

    results: list[IndexSchemaDiff] = []
    for name in common:
        li = left_map[name]
        ri = right_map[name]
        if li.sql == ri.sql:
            continue
        results.append(IndexSchemaDiff(name=name, old_sql=li.sql, new_sql=ri.sql))
    return tuple(results)


# ---------------------------------------------------------------------------
# Trigger diffing
# ---------------------------------------------------------------------------


def _diff_triggers(
    left: tuple[TriggerInfo, ...], right: tuple[TriggerInfo, ...]
) -> tuple[TriggerSchemaDiff, ...]:
    """Find triggers present in both databases whose SQL differs."""
    left_map = _by_name(left)
    right_map = _by_name(right)
    common = sorted(set(left_map) & set(right_map))

    results: list[TriggerSchemaDiff] = []
    for name in common:
        lt = left_map[name]
        rt = right_map[name]
        if lt.sql == rt.sql:
            continue
        results.append(TriggerSchemaDiff(name=name, old_sql=lt.sql, new_sql=rt.sql))
    return tuple(results)


# ---------------------------------------------------------------------------
# View diffing
# ---------------------------------------------------------------------------


def _diff_views(
    left: tuple[ViewInfo, ...], right: tuple[ViewInfo, ...]
) -> tuple[ViewSchemaDiff, ...]:
    """Find views present in both databases whose SQL differs."""
    left_map = _by_name(left)
    right_map = _by_name(right)
    common = sorted(set(left_map) & set(right_map))

    results: list[ViewSchemaDiff] = []
    for name in common:
        lv = left_map[name]
        rv = right_map[name]
        if lv.sql == rv.sql:
            continue
        results.append(ViewSchemaDiff(name=name, old_sql=lv.sql, new_sql=rv.sql))
    return tuple(results)
