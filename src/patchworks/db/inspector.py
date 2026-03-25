"""SQLite database inspection, schema reading, and row pagination.

Opens databases in **read-only** mode via ``file:`` URIs and handles
WAL-mode journals transparently.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Iterator

from patchworks.db.types import (
    ColumnInfo,
    DatabaseSummary,
    IndexInfo,
    TableInfo,
    TriggerInfo,
    ViewInfo,
)

# ---------------------------------------------------------------------------
# Connection helpers
# ---------------------------------------------------------------------------

_DEFAULT_PAGE_SIZE: int = 1000


def _open_readonly(path: str | Path) -> sqlite3.Connection:
    """Open a SQLite database in read-only mode.

    Uses the ``file:`` URI scheme with ``?mode=ro`` so SQLite never creates
    or modifies the file.  ``immutable=0`` allows reading WAL-mode databases
    (the OS-level file may have a ``-wal`` sidecar).
    """
    resolved = Path(path).resolve()
    uri = f"file:{resolved}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    conn.row_factory = sqlite3.Row
    return conn


# ---------------------------------------------------------------------------
# Low-level schema queries
# ---------------------------------------------------------------------------


def _read_columns(conn: sqlite3.Connection, table_name: str) -> tuple[ColumnInfo, ...]:
    """Return column metadata for *table_name* via ``PRAGMA table_info``."""
    rows = conn.execute(f"PRAGMA table_info({_qi(table_name)})").fetchall()
    return tuple(
        ColumnInfo(
            name=row["name"],
            type=row["type"],
            notnull=bool(row["notnull"]),
            default_value=row["dflt_value"],
            primary_key=row["pk"],
        )
        for row in rows
    )


def _read_indexes(
    conn: sqlite3.Connection, table_name: str | None = None
) -> tuple[IndexInfo, ...]:
    """Return index metadata, optionally filtered to *table_name*."""
    if table_name is not None:
        idx_rows = conn.execute(f"PRAGMA index_list({_qi(table_name)})").fetchall()
    else:
        # Collect indexes across all tables.
        tables = [
            r["name"]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' "
                "AND name NOT LIKE 'sqlite_%'"
            ).fetchall()
        ]
        idx_rows_all: list[sqlite3.Row] = []
        for tbl in tables:
            idx_rows_all.extend(
                conn.execute(f"PRAGMA index_list({_qi(tbl)})").fetchall()
            )
        idx_rows = idx_rows_all  # type: ignore[assignment]

    results: list[IndexInfo] = []
    for row in idx_rows:
        idx_name: str = row["name"]
        # index_info gives us the columns in the index.
        cols_rows = conn.execute(f"PRAGMA index_info({_qi(idx_name)})").fetchall()
        col_names = tuple(r["name"] for r in cols_rows)

        # Get the SQL and table from sqlite_master.
        master_row = conn.execute(
            "SELECT sql, tbl_name FROM sqlite_master WHERE type='index' AND name=?",
            (idx_name,),
        ).fetchone()

        sql: str | None = None
        tbl: str = ""
        try:
            partial = bool(row["partial"])
        except IndexError:
            partial = False
        if master_row is not None:
            sql = master_row["sql"]
            tbl = master_row["tbl_name"]
        else:
            # Auto-indexes (e.g. for UNIQUE constraints) may not appear in
            # sqlite_master.  Fall back to the table we were querying.
            tbl = table_name or ""

        results.append(
            IndexInfo(
                name=idx_name,
                table_name=tbl,
                unique=bool(row["unique"]),
                columns=col_names,
                partial=partial,
                sql=sql,
            )
        )
    return tuple(results)


def _read_triggers(
    conn: sqlite3.Connection, table_name: str | None = None
) -> tuple[TriggerInfo, ...]:
    """Return trigger metadata, optionally filtered to *table_name*."""
    if table_name is not None:
        rows = conn.execute(
            "SELECT name, tbl_name, sql FROM sqlite_master "
            "WHERE type='trigger' AND tbl_name=?",
            (table_name,),
        ).fetchall()
    else:
        rows = conn.execute(
            "SELECT name, tbl_name, sql FROM sqlite_master WHERE type='trigger'"
        ).fetchall()
    return tuple(
        TriggerInfo(name=r["name"], table_name=r["tbl_name"], sql=r["sql"])
        for r in rows
    )


def _read_views(conn: sqlite3.Connection) -> tuple[ViewInfo, ...]:
    """Return metadata for all views."""
    rows = conn.execute(
        "SELECT name, sql FROM sqlite_master WHERE type='view' ORDER BY name"
    ).fetchall()
    results: list[ViewInfo] = []
    for row in rows:
        name: str = row["name"]
        cols = _read_columns(conn, name)
        results.append(ViewInfo(name=name, columns=cols, sql=row["sql"]))
    return tuple(results)


def _is_without_rowid(conn: sqlite3.Connection, table_name: str) -> bool:
    """Heuristically detect ``WITHOUT ROWID`` tables.

    SQLite does not expose this in any PRAGMA.  The most reliable method is
    to attempt a ``SELECT rowid`` and catch the resulting error.
    """
    try:
        conn.execute(f"SELECT rowid FROM {_qi(table_name)} LIMIT 0")
        return False
    except sqlite3.OperationalError:
        return True


def _primary_key_columns(columns: tuple[ColumnInfo, ...]) -> tuple[str, ...]:
    """Extract PK column names in PK-positional order."""
    pk_cols = [(c.name, c.primary_key) for c in columns if c.primary_key > 0]
    pk_cols.sort(key=lambda t: t[1])
    return tuple(name for name, _ in pk_cols)


# ---------------------------------------------------------------------------
# Row pagination
# ---------------------------------------------------------------------------


def _order_clause(pk_columns: tuple[str, ...], without_rowid: bool) -> str:
    """Build a deterministic ``ORDER BY`` clause.

    For tables with an explicit PK, order by those columns.  For rowid
    tables without an explicit PK, fall back to ``rowid``.
    """
    if pk_columns:
        return "ORDER BY " + ", ".join(_qi(c) for c in pk_columns)
    if not without_rowid:
        return "ORDER BY rowid"
    # WITHOUT ROWID with no explicit PK shouldn't happen (SQLite requires
    # a PK for WITHOUT ROWID), but guard defensively.
    return ""


def read_rows(
    conn: sqlite3.Connection,
    table_name: str,
    *,
    page_size: int = _DEFAULT_PAGE_SIZE,
    offset: int = 0,
    pk_columns: tuple[str, ...] = (),
    without_rowid: bool = False,
) -> list[dict[str, Any]]:
    """Read a single page of rows from *table_name*.

    Returns up to *page_size* rows starting at *offset*.  Rows are returned
    as dictionaries keyed by column name.
    """
    order = _order_clause(pk_columns, without_rowid)
    query = f"SELECT * FROM {_qi(table_name)} {order} LIMIT ? OFFSET ?"
    cursor = conn.execute(query, (page_size, offset))
    col_names = [desc[0] for desc in cursor.description or []]
    return [dict(zip(col_names, row, strict=True)) for row in cursor.fetchall()]


def for_each_row(
    conn: sqlite3.Connection,
    table_name: str,
    *,
    page_size: int = _DEFAULT_PAGE_SIZE,
    pk_columns: tuple[str, ...] = (),
    without_rowid: bool = False,
) -> Iterator[dict[str, Any]]:
    """Stream all rows from *table_name* with bounded memory.

    Yields one row at a time, fetching *page_size* rows per internal query.
    """
    offset = 0
    while True:
        page = read_rows(
            conn,
            table_name,
            page_size=page_size,
            offset=offset,
            pk_columns=pk_columns,
            without_rowid=without_rowid,
        )
        if not page:
            break
        yield from page
        offset += len(page)


# ---------------------------------------------------------------------------
# High-level inspection
# ---------------------------------------------------------------------------


def inspect_table(conn: sqlite3.Connection, table_name: str) -> TableInfo:
    """Build a :class:`TableInfo` for *table_name*."""
    columns = _read_columns(conn, table_name)
    pk_cols = _primary_key_columns(columns)
    wo_rowid = _is_without_rowid(conn, table_name)
    indexes = _read_indexes(conn, table_name)
    triggers = _read_triggers(conn, table_name)

    row_count_row = conn.execute(
        f"SELECT COUNT(*) AS cnt FROM {_qi(table_name)}"
    ).fetchone()
    row_count: int = row_count_row["cnt"] if row_count_row else 0

    master = conn.execute(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
        (table_name,),
    ).fetchone()
    sql: str = master["sql"] if master else ""

    return TableInfo(
        name=table_name,
        columns=columns,
        primary_key_columns=pk_cols,
        without_rowid=wo_rowid,
        row_count=row_count,
        indexes=indexes,
        triggers=triggers,
        sql=sql,
    )


def inspect_database(path: str | Path) -> DatabaseSummary:
    """Return a complete :class:`DatabaseSummary` for the SQLite file at *path*.

    The database is opened read-only and closed before returning.
    """
    conn = _open_readonly(path)
    try:
        # Pragmas -----------------------------------------------------------
        page_size: int = conn.execute("PRAGMA page_size").fetchone()["page_size"]
        page_count: int = conn.execute("PRAGMA page_count").fetchone()["page_count"]
        journal_mode: str = conn.execute("PRAGMA journal_mode").fetchone()[
            "journal_mode"
        ]

        # Tables ------------------------------------------------------------
        table_names = [
            r["name"]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' "
                "AND name NOT LIKE 'sqlite_%' ORDER BY name"
            ).fetchall()
        ]
        tables = tuple(inspect_table(conn, n) for n in table_names)

        # Views -------------------------------------------------------------
        views = _read_views(conn)

        # All indexes and triggers (across all tables) ----------------------
        all_indexes = _read_indexes(conn)
        all_triggers = _read_triggers(conn)

        return DatabaseSummary(
            path=str(Path(path).resolve()),
            page_size=page_size,
            page_count=page_count,
            journal_mode=journal_mode,
            tables=tables,
            views=views,
            indexes=all_indexes,
            triggers=all_triggers,
        )
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# Identifier quoting helper
# ---------------------------------------------------------------------------


def _qi(identifier: str) -> str:
    """Quote a SQL identifier to prevent injection."""
    return '"' + identifier.replace('"', '""') + '"'
