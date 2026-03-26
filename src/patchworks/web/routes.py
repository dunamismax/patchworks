"""Web UI routes for patchworks.

All routes call the same backend functions as the CLI — no forked logic.
htmx partial responses use the ``HX-Request`` header to distinguish full
page loads from htmx fragment requests.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from fastapi import APIRouter, Form, Query, Request
from fastapi.responses import HTMLResponse, PlainTextResponse

from patchworks.db.inspector import (
    _open_readonly,
    inspect_database,
    read_rows,
)
from patchworks.db.snapshot import SnapshotStore
from patchworks.diff.export import export_as_sql
from patchworks.diff.semantic import summarize_diff
from patchworks.web.app import templates

if TYPE_CHECKING:
    from patchworks.db.types import DatabaseSummary, TableInfo

router = APIRouter()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_PAGE_SIZE = 50


def _is_htmx(request: Request) -> bool:
    return request.headers.get("HX-Request") == "true"


def _render(request: Request, template_name: str, **extra: Any) -> HTMLResponse:
    """Render a Jinja2 template with *request* in the context."""
    ctx: dict[str, Any] = {"request": request, **extra}
    return templates.TemplateResponse(request, template_name, ctx)  # type: ignore[return-value]


def _safe_inspect(path: str) -> DatabaseSummary | None:
    """Inspect a database, returning ``None`` on error."""
    p = Path(path)
    if not p.exists():
        return None
    try:
        return inspect_database(p)
    except Exception:
        return None


def _get_table(summary: DatabaseSummary, name: str) -> TableInfo | None:
    for t in summary.tables:
        if t.name == name:
            return t
    return None


# ---------------------------------------------------------------------------
# Index
# ---------------------------------------------------------------------------


@router.get("/", response_class=HTMLResponse)
async def index(request: Request) -> HTMLResponse:
    return _render(request, "index.html")


# ---------------------------------------------------------------------------
# Schema browser
# ---------------------------------------------------------------------------


@router.get("/browse", response_class=HTMLResponse)
async def browse_form(request: Request) -> HTMLResponse:
    return _render(request, "browse.html")


@router.get("/browse/schema", response_class=HTMLResponse)
async def browse_schema(
    request: Request,
    db: str = Query(""),
) -> HTMLResponse:
    if not db:
        return _render(
            request,
            "partials/error.html",
            error="No database path provided.",
        )
    summary = _safe_inspect(db)
    if summary is None:
        return _render(
            request,
            "partials/error.html",
            error=f"Cannot open database: {db}",
        )
    template = "partials/schema.html" if _is_htmx(request) else "browse.html"
    return _render(request, template, summary=summary, db=db)


@router.get("/browse/ddl", response_class=HTMLResponse)
async def browse_ddl(
    request: Request,
    db: str = Query(""),
    name: str = Query(""),
    kind: str = Query("table"),
) -> HTMLResponse:
    """Return DDL preview for a schema object."""
    if not db or not name:
        return _render(
            request,
            "partials/error.html",
            error="Missing db or object name.",
        )
    summary = _safe_inspect(db)
    if summary is None:
        return _render(
            request,
            "partials/error.html",
            error=f"Cannot open database: {db}",
        )
    sql = ""
    if kind == "table":
        for t in summary.tables:
            if t.name == name:
                sql = t.sql
                break
    elif kind == "view":
        for v in summary.views:
            if v.name == name:
                sql = v.sql
                break
    elif kind == "index":
        for i in summary.indexes:
            if i.name == name:
                sql = i.sql or "(auto-index)"
                break
    elif kind == "trigger":
        for tr in summary.triggers:
            if tr.name == name:
                sql = tr.sql
                break
    return _render(request, "partials/ddl.html", sql=sql, name=name, kind=kind)


# ---------------------------------------------------------------------------
# Table row browser
# ---------------------------------------------------------------------------


@router.get("/browse/rows", response_class=HTMLResponse)
async def browse_rows(
    request: Request,
    db: str = Query(""),
    table: str = Query(""),
    page: int = Query(1, ge=1),
) -> HTMLResponse:
    if not db or not table:
        return _render(
            request,
            "partials/error.html",
            error="Missing db or table name.",
        )
    summary = _safe_inspect(db)
    if summary is None:
        return _render(
            request,
            "partials/error.html",
            error=f"Cannot open database: {db}",
        )
    tinfo = _get_table(summary, table)
    if tinfo is None:
        return _render(
            request,
            "partials/error.html",
            error=f"Table not found: {table}",
        )
    offset = (page - 1) * _PAGE_SIZE
    conn = _open_readonly(db)
    try:
        rows = read_rows(
            conn,
            table,
            page_size=_PAGE_SIZE,
            offset=offset,
            pk_columns=tinfo.primary_key_columns,
            without_rowid=tinfo.without_rowid,
        )
    finally:
        conn.close()

    total_pages = max(1, (tinfo.row_count + _PAGE_SIZE - 1) // _PAGE_SIZE)
    columns = [c.name for c in tinfo.columns]

    template = "partials/rows.html" if _is_htmx(request) else "rows.html"
    return _render(
        request,
        template,
        db=db,
        table=table,
        columns=columns,
        rows=rows,
        page=page,
        total_pages=total_pages,
        row_count=tinfo.row_count,
    )


# ---------------------------------------------------------------------------
# Diff viewer
# ---------------------------------------------------------------------------


@router.get("/diff", response_class=HTMLResponse)
async def diff_form(request: Request) -> HTMLResponse:
    return _render(request, "diff.html")


@router.get("/diff/result", response_class=HTMLResponse)
async def diff_result(
    request: Request,
    left: str = Query(""),
    right: str = Query(""),
) -> HTMLResponse:
    if not left or not right:
        return _render(
            request,
            "partials/error.html",
            error="Both database paths are required.",
        )
    for p, label in ((left, "left"), (right, "right")):
        if not Path(p).exists():
            return _render(
                request,
                "partials/error.html",
                error=f"{label} database not found: {p}",
            )
    try:
        from patchworks.db.differ import diff_databases

        result = diff_databases(left, right)
    except Exception as exc:
        return _render(
            request,
            "partials/error.html",
            error=str(exc),
        )

    summary = summarize_diff(result)
    template = "partials/diff.html" if _is_htmx(request) else "diff.html"
    return _render(
        request,
        template,
        diff=result,
        summary=summary,
        left=left,
        right=right,
    )


# ---------------------------------------------------------------------------
# SQL export preview
# ---------------------------------------------------------------------------


@router.get("/export", response_class=HTMLResponse)
async def export_form(request: Request) -> HTMLResponse:
    return _render(request, "export.html")


@router.get("/export/preview", response_class=HTMLResponse)
async def export_preview(
    request: Request,
    left: str = Query(""),
    right: str = Query(""),
) -> HTMLResponse:
    if not left or not right:
        return _render(
            request,
            "partials/error.html",
            error="Both database paths are required.",
        )
    for p, label in ((left, "left"), (right, "right")):
        if not Path(p).exists():
            return _render(
                request,
                "partials/error.html",
                error=f"{label} database not found: {p}",
            )
    try:
        from patchworks.db.differ import diff_databases

        result = diff_databases(left, right)
        sql = export_as_sql(result, right_path=right)
    except Exception as exc:
        return _render(
            request,
            "partials/error.html",
            error=str(exc),
        )

    template = "partials/export.html" if _is_htmx(request) else "export.html"
    return _render(request, template, sql=sql, left=left, right=right)


@router.get("/export/download", response_class=PlainTextResponse)
async def export_download(
    left: str = Query(""),
    right: str = Query(""),
) -> PlainTextResponse:
    """Download the migration SQL as a plain text file."""
    if not left or not right:
        return PlainTextResponse(
            "Both database paths are required.",
            status_code=400,
        )
    for p in (left, right):
        if not Path(p).exists():
            return PlainTextResponse(
                f"Database not found: {p}",
                status_code=404,
            )
    try:
        from patchworks.db.differ import diff_databases

        result = diff_databases(left, right)
        sql = export_as_sql(result, right_path=right)
    except Exception as exc:
        return PlainTextResponse(str(exc), status_code=500)

    return PlainTextResponse(
        sql,
        media_type="application/sql",
        headers={
            "Content-Disposition": "attachment; filename=migration.sql",
        },
    )


# ---------------------------------------------------------------------------
# Snapshot management
# ---------------------------------------------------------------------------


@router.get("/snapshots", response_class=HTMLResponse)
async def snapshots_page(request: Request) -> HTMLResponse:
    try:
        store = SnapshotStore()
        snaps = store.list()
    except Exception:
        snaps = []
    return _render(request, "snapshots.html", snapshots=snaps)


@router.get("/snapshots/list", response_class=HTMLResponse)
async def snapshots_list(request: Request) -> HTMLResponse:
    """htmx partial: refresh snapshot list."""
    try:
        store = SnapshotStore()
        snaps = store.list()
    except Exception:
        snaps = []
    return _render(request, "partials/snapshot_list.html", snapshots=snaps)


@router.post("/snapshots/save", response_class=HTMLResponse)
async def snapshot_save(
    request: Request,
    database: str = Form(""),
    name: str = Form(""),
) -> HTMLResponse:
    if not database:
        return _render(
            request,
            "partials/error.html",
            error="Database path is required.",
        )
    if not Path(database).exists():
        return _render(
            request,
            "partials/error.html",
            error=f"Database not found: {database}",
        )
    try:
        store = SnapshotStore()
        store.save(database, name=name or None)
        snaps = store.list()
    except Exception as exc:
        return _render(
            request,
            "partials/error.html",
            error=str(exc),
        )
    return _render(request, "partials/snapshot_list.html", snapshots=snaps)


@router.delete("/snapshots/{snap_id}", response_class=HTMLResponse)
async def snapshot_delete(request: Request, snap_id: str) -> HTMLResponse:
    try:
        store = SnapshotStore()
        store.delete(snap_id)
        snaps = store.list()
    except Exception as exc:
        return _render(
            request,
            "partials/error.html",
            error=str(exc),
        )
    return _render(request, "partials/snapshot_list.html", snapshots=snaps)
