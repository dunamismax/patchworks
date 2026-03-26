"""FastAPI application factory for the patchworks local web UI.

The web UI is a thin layer over the same backend functions the CLI uses.
No forked logic — routes call ``inspect_database``, ``diff_databases``,
``SnapshotStore``, ``export_as_sql``, etc. directly.
"""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates

_WEB_DIR = Path(__file__).parent
_TEMPLATES_DIR = _WEB_DIR / "templates"
_STATIC_DIR = _WEB_DIR / "static"

templates = Jinja2Templates(directory=str(_TEMPLATES_DIR))


def create_app() -> FastAPI:
    """Build and return the FastAPI application."""
    from patchworks.web.routes import router

    app = FastAPI(title="patchworks", docs_url=None, redoc_url=None)
    app.mount("/static", StaticFiles(directory=str(_STATIC_DIR)), name="static")
    app.include_router(router)
    return app
