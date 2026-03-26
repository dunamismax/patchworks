"""Semantic diff awareness: renames, type shifts, and intelligent comparison.

Layers heuristic detection on top of raw :class:`DatabaseDiff` results.
All detections carry confidence scores so consumers can threshold to taste.
"""

from __future__ import annotations

from typing import Any

from patchworks.db.types import (
    AnnotationStatus,
    CellChange,
    ColumnInfo,
    ColumnRenameCandidate,
    DatabaseDiff,
    DiffAnnotation,
    DiffSummary,
    RowDiff,
    SchemaDiff,
    SemanticAnalysis,
    TableDataDiff,
    TableInfo,
    TableRenameCandidate,
    TypeShift,
)

# ---------------------------------------------------------------------------
# SQLite type affinity (https://www.sqlite.org/datatype3.html §3.1)
# ---------------------------------------------------------------------------

_AFFINITY_INT = "INTEGER"
_AFFINITY_TEXT = "TEXT"
_AFFINITY_BLOB = "BLOB"
_AFFINITY_REAL = "REAL"
_AFFINITY_NUMERIC = "NUMERIC"


def sqlite_type_affinity(declared: str) -> str:
    """Determine the SQLite type affinity for a declared column type.

    Follows the five rules in https://www.sqlite.org/datatype3.html §3.1.
    """
    upper = declared.upper().strip()
    if not upper:
        return _AFFINITY_BLOB

    # Rule 1: contains "INT"
    if "INT" in upper:
        return _AFFINITY_INT

    # Rule 2: contains "CHAR", "CLOB", or "TEXT"
    if any(k in upper for k in ("CHAR", "CLOB", "TEXT")):
        return _AFFINITY_TEXT

    # Rule 3: contains "BLOB" or is empty (already handled)
    if "BLOB" in upper:
        return _AFFINITY_BLOB

    # Rule 4: contains "REAL", "FLOA", or "DOUB"
    if any(k in upper for k in ("REAL", "FLOA", "DOUB")):
        return _AFFINITY_REAL

    # Rule 5: everything else
    return _AFFINITY_NUMERIC


# ---------------------------------------------------------------------------
# Affinity compatibility matrix
# ---------------------------------------------------------------------------

# Shifts that preserve data with no lossy coercion.
_COMPATIBLE_SHIFTS: set[tuple[str, str]] = {
    (_AFFINITY_INT, _AFFINITY_REAL),
    (_AFFINITY_INT, _AFFINITY_NUMERIC),
    (_AFFINITY_REAL, _AFFINITY_NUMERIC),
    (_AFFINITY_NUMERIC, _AFFINITY_INT),
    (_AFFINITY_NUMERIC, _AFFINITY_REAL),
    # TEXT ↔ NUMERIC is lossless at the storage level in SQLite.
    (_AFFINITY_NUMERIC, _AFFINITY_TEXT),
    (_AFFINITY_TEXT, _AFFINITY_NUMERIC),
    # BLOB ↔ BLOB is always compatible (same affinity).
}


def is_affinity_compatible(old_affinity: str, new_affinity: str) -> bool:
    """Return ``True`` if *old_affinity* → *new_affinity* is lossless."""
    if old_affinity == new_affinity:
        return True
    return (old_affinity, new_affinity) in _COMPATIBLE_SHIFTS


# ---------------------------------------------------------------------------
# Data-type-aware value comparison
# ---------------------------------------------------------------------------


def values_semantically_equal(old: Any, new: Any) -> bool:
    """Return ``True`` if *old* and *new* are semantically equivalent.

    Handles:
    - integer 1 vs real 1.0
    - text "42" vs integer 42 / real 42.0
    - None == None
    """
    if old is None and new is None:
        return True
    if old is None or new is None:
        return False

    # Identical values (including type).
    if old == new:
        return True

    # int / float equivalence: 1 == 1.0
    if isinstance(old, (int, float)) and isinstance(new, (int, float)):
        try:
            return float(old) == float(new)
        except (ValueError, OverflowError):
            return False

    # text ↔ numeric: "42" == 42 or "42.0" == 42.0
    if isinstance(old, str) and isinstance(new, (int, float)):
        return _str_numeric_eq(old, new)
    if isinstance(new, str) and isinstance(old, (int, float)):
        return _str_numeric_eq(new, old)

    return False


def _str_numeric_eq(s: str, n: int | float) -> bool:
    """Return ``True`` if string *s* represents the same numeric value as *n*."""
    try:
        f = float(s)
        return f == float(n)
    except (ValueError, OverflowError):
        return False


# ---------------------------------------------------------------------------
# Table rename detection
# ---------------------------------------------------------------------------


def _column_names(table: TableInfo) -> set[str]:
    return {c.name for c in table.columns}


def _column_similarity(left: TableInfo, right: TableInfo) -> float:
    """Jaccard similarity of column name sets."""
    l_names = _column_names(left)
    r_names = _column_names(right)
    if not l_names and not r_names:
        return 1.0
    if not l_names or not r_names:
        return 0.0
    intersection = l_names & r_names
    union = l_names | r_names
    return len(intersection) / len(union)


def detect_table_renames(
    schema_diff: SchemaDiff,
    *,
    threshold: float = 0.6,
) -> tuple[TableRenameCandidate, ...]:
    """Detect probable table renames from added/removed table pairs.

    Each removed table is matched against each added table by column
    similarity.  Only pairs scoring above *threshold* are returned.
    Greedy best-match pairing ensures one-to-one mapping.
    """
    if not schema_diff.tables_removed or not schema_diff.tables_added:
        return ()

    # Score all pairs.
    scores: list[tuple[float, TableInfo, TableInfo]] = []
    for removed in schema_diff.tables_removed:
        for added in schema_diff.tables_added:
            sim = _column_similarity(removed, added)
            if sim >= threshold:
                scores.append((sim, removed, added))

    # Greedy best-match.
    scores.sort(key=lambda x: x[0], reverse=True)
    used_removed: set[str] = set()
    used_added: set[str] = set()
    results: list[TableRenameCandidate] = []

    for sim, removed, added in scores:
        if removed.name in used_removed or added.name in used_added:
            continue
        matched = tuple(sorted(_column_names(removed) & _column_names(added)))
        results.append(
            TableRenameCandidate(
                old_name=removed.name,
                new_name=added.name,
                confidence=round(sim, 4),
                matched_columns=matched,
            )
        )
        used_removed.add(removed.name)
        used_added.add(added.name)

    return tuple(results)


# ---------------------------------------------------------------------------
# Column rename detection
# ---------------------------------------------------------------------------


def _column_property_match_score(old: ColumnInfo, new: ColumnInfo) -> float:
    """Score how closely two columns match on non-name properties.

    Properties checked: type, notnull, primary_key, default_value.
    Each match earns 0.25.
    """
    score = 0.0
    if old.type.upper() == new.type.upper():
        score += 0.25
    elif sqlite_type_affinity(old.type) == sqlite_type_affinity(new.type):
        score += 0.15  # Same affinity but different declared type.
    if old.notnull == new.notnull:
        score += 0.25
    if old.primary_key == new.primary_key:
        score += 0.25
    if old.default_value == new.default_value:
        score += 0.25
    return score


def detect_column_renames(
    schema_diff: SchemaDiff,
    *,
    threshold: float = 0.7,
) -> tuple[ColumnRenameCandidate, ...]:
    """Detect probable column renames within modified tables.

    Pairs each removed column with each added column by property
    matching.  Only pairs scoring above *threshold* are returned.
    """
    results: list[ColumnRenameCandidate] = []

    for table_mod in schema_diff.tables_modified:
        if not table_mod.columns_removed or not table_mod.columns_added:
            continue

        scores: list[tuple[float, ColumnInfo, ColumnInfo]] = []
        for removed in table_mod.columns_removed:
            for added in table_mod.columns_added:
                sc = _column_property_match_score(removed, added)
                if sc >= threshold:
                    scores.append((sc, removed, added))

        scores.sort(key=lambda x: x[0], reverse=True)
        used_old: set[str] = set()
        used_new: set[str] = set()

        for sc, old_col, new_col in scores:
            if old_col.name in used_old or new_col.name in used_new:
                continue
            results.append(
                ColumnRenameCandidate(
                    table_name=table_mod.table_name,
                    old_name=old_col.name,
                    new_name=new_col.name,
                    confidence=round(sc, 4),
                )
            )
            used_old.add(old_col.name)
            used_new.add(new_col.name)

    return tuple(results)


# ---------------------------------------------------------------------------
# Type shift detection
# ---------------------------------------------------------------------------


def detect_type_shifts(
    schema_diff: SchemaDiff,
) -> tuple[TypeShift, ...]:
    """Detect type shifts in modified columns and classify compatibility."""
    results: list[TypeShift] = []

    for table_mod in schema_diff.tables_modified:
        for old_col, new_col in table_mod.columns_modified:
            if old_col.type.upper() == new_col.type.upper():
                continue

            old_aff = sqlite_type_affinity(old_col.type)
            new_aff = sqlite_type_affinity(new_col.type)
            compatible = is_affinity_compatible(old_aff, new_aff)

            results.append(
                TypeShift(
                    table_name=table_mod.table_name,
                    column_name=old_col.name,
                    old_type=old_col.type,
                    new_type=new_col.type,
                    old_affinity=old_aff,
                    new_affinity=new_aff,
                    compatible=compatible,
                    confidence=1.0,
                )
            )

    return tuple(results)


# ---------------------------------------------------------------------------
# Diff filtering
# ---------------------------------------------------------------------------


def filter_diff(
    diff: DatabaseDiff,
    *,
    change_types: set[str] | None = None,
    tables: set[str] | None = None,
) -> DatabaseDiff:
    """Return a copy of *diff* filtered by change type and/or table name.

    *change_types* can include ``"added"``, ``"removed"``, ``"modified"``.
    *tables* restricts output to the named tables only.
    """
    sd = diff.schema_diff

    # --- Schema filtering ---
    new_tables_added = sd.tables_added
    new_tables_removed = sd.tables_removed
    new_tables_modified = sd.tables_modified
    new_indexes_added = sd.indexes_added
    new_indexes_removed = sd.indexes_removed
    new_indexes_modified = sd.indexes_modified
    new_triggers_added = sd.triggers_added
    new_triggers_removed = sd.triggers_removed
    new_triggers_modified = sd.triggers_modified
    new_views_added = sd.views_added
    new_views_removed = sd.views_removed
    new_views_modified = sd.views_modified

    if change_types is not None:
        if "added" not in change_types:
            new_tables_added = ()
            new_indexes_added = ()
            new_triggers_added = ()
            new_views_added = ()
        if "removed" not in change_types:
            new_tables_removed = ()
            new_indexes_removed = ()
            new_triggers_removed = ()
            new_views_removed = ()
        if "modified" not in change_types:
            new_tables_modified = ()
            new_indexes_modified = ()
            new_triggers_modified = ()
            new_views_modified = ()

    if tables is not None:
        new_tables_added = tuple(t for t in new_tables_added if t.name in tables)
        new_tables_removed = tuple(t for t in new_tables_removed if t.name in tables)
        new_tables_modified = tuple(
            t for t in new_tables_modified if t.table_name in tables
        )
        new_indexes_added = tuple(
            i for i in new_indexes_added if i.table_name in tables
        )
        new_indexes_removed = tuple(
            i for i in new_indexes_removed if i.table_name in tables
        )
        new_triggers_added = tuple(
            t for t in new_triggers_added if t.table_name in tables
        )
        new_triggers_removed = tuple(
            t for t in new_triggers_removed if t.table_name in tables
        )

    new_schema = SchemaDiff(
        tables_added=new_tables_added,
        tables_removed=new_tables_removed,
        tables_modified=new_tables_modified,
        indexes_added=new_indexes_added,
        indexes_removed=new_indexes_removed,
        indexes_modified=new_indexes_modified,
        triggers_added=new_triggers_added,
        triggers_removed=new_triggers_removed,
        triggers_modified=new_triggers_modified,
        views_added=new_views_added,
        views_removed=new_views_removed,
        views_modified=new_views_modified,
    )

    # --- Data filtering ---
    new_data_diffs: list[TableDataDiff] = []
    for td in diff.table_data_diffs:
        if tables is not None and td.table_name not in tables:
            continue

        if change_types is not None:
            filtered_diffs: list[RowDiff] = []
            for rd in td.row_diffs:
                if rd.kind in change_types:
                    filtered_diffs.append(rd)

            added = sum(1 for r in filtered_diffs if r.kind == "added")
            removed = sum(1 for r in filtered_diffs if r.kind == "removed")
            modified = sum(1 for r in filtered_diffs if r.kind == "modified")

            new_data_diffs.append(
                TableDataDiff(
                    table_name=td.table_name,
                    key_columns=td.key_columns,
                    rows_added=added,
                    rows_removed=removed,
                    rows_modified=modified,
                    row_diffs=tuple(filtered_diffs),
                    warnings=td.warnings,
                )
            )
        else:
            new_data_diffs.append(td)

    return DatabaseDiff(
        left_path=diff.left_path,
        right_path=diff.right_path,
        schema_diff=new_schema,
        table_data_diffs=tuple(new_data_diffs),
        warnings=diff.warnings,
    )


# ---------------------------------------------------------------------------
# Aggregate diff summary
# ---------------------------------------------------------------------------


def summarize_diff(diff: DatabaseDiff) -> DiffSummary:
    """Compute aggregate statistics for a :class:`DatabaseDiff`."""
    sd = diff.schema_diff

    total_cell_changes = 0
    total_added = 0
    total_removed = 0
    total_modified = 0

    for td in diff.table_data_diffs:
        total_added += td.rows_added
        total_removed += td.rows_removed
        total_modified += td.rows_modified
        for rd in td.row_diffs:
            total_cell_changes += len(rd.cell_changes)

    return DiffSummary(
        tables_added=len(sd.tables_added),
        tables_removed=len(sd.tables_removed),
        tables_modified=len(sd.tables_modified),
        indexes_added=len(sd.indexes_added),
        indexes_removed=len(sd.indexes_removed),
        indexes_modified=len(sd.indexes_modified),
        triggers_added=len(sd.triggers_added),
        triggers_removed=len(sd.triggers_removed),
        triggers_modified=len(sd.triggers_modified),
        views_added=len(sd.views_added),
        views_removed=len(sd.views_removed),
        views_modified=len(sd.views_modified),
        total_rows_added=total_added,
        total_rows_removed=total_removed,
        total_rows_modified=total_modified,
        total_cell_changes=total_cell_changes,
    )


# ---------------------------------------------------------------------------
# Data-type-aware cell comparison
# ---------------------------------------------------------------------------


def semantic_cell_changes(
    old: dict[str, Any], new: dict[str, Any]
) -> tuple[CellChange, ...]:
    """Like ``_cell_changes`` but uses data-type-aware comparison.

    Returns only the changes where values are *semantically* different
    (e.g. integer ``1`` vs real ``1.0`` are considered equal).
    """
    changes: list[CellChange] = []
    all_keys = dict.fromkeys([*old.keys(), *new.keys()])
    for col in all_keys:
        old_val = old.get(col)
        new_val = new.get(col)
        if not values_semantically_equal(old_val, new_val):
            changes.append(CellChange(column=col, old_value=old_val, new_value=new_val))
    return tuple(changes)


# ---------------------------------------------------------------------------
# Annotations
# ---------------------------------------------------------------------------


def annotate(
    existing: tuple[DiffAnnotation, ...],
    target: str,
    status: AnnotationStatus,
    note: str = "",
) -> tuple[DiffAnnotation, ...]:
    """Add or update an annotation for *target*.

    If an annotation for *target* already exists it is replaced.
    """
    filtered = tuple(a for a in existing if a.target != target)
    return (*filtered, DiffAnnotation(target=target, status=status, note=note))


# ---------------------------------------------------------------------------
# Full semantic analysis
# ---------------------------------------------------------------------------


def analyze(
    diff: DatabaseDiff,
    *,
    rename_threshold: float = 0.6,
    column_rename_threshold: float = 0.7,
) -> SemanticAnalysis:
    """Run all semantic detectors on a :class:`DatabaseDiff`.

    Returns a :class:`SemanticAnalysis` combining rename candidates,
    type shifts, and aggregate summary.
    """
    table_renames = detect_table_renames(diff.schema_diff, threshold=rename_threshold)
    column_renames = detect_column_renames(
        diff.schema_diff, threshold=column_rename_threshold
    )
    type_shifts = detect_type_shifts(diff.schema_diff)
    summary = summarize_diff(diff)

    return SemanticAnalysis(
        table_renames=table_renames,
        column_renames=column_renames,
        type_shifts=type_shifts,
        summary=summary,
    )
