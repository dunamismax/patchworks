"""Smoke tests for the patchworks web UI routes.

Each page should return HTTP 200.  Tests use FastAPI's TestClient (httpx)
and ephemeral SQLite databases to verify the routes call the same backend
functions as the CLI.
"""

from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

import pytest
from fastapi.testclient import TestClient

if TYPE_CHECKING:
    from pathlib import Path

from patchworks.db.snapshot import SnapshotStore
from patchworks.web.app import create_app


@pytest.fixture()
def client() -> TestClient:
    app = create_app()
    return TestClient(app)


@pytest.fixture()
def sample_db(tmp_path: Path) -> Path:
    """Create a simple SQLite database for testing."""
    db = tmp_path / "test.sqlite"
    conn = sqlite3.connect(str(db))
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
    conn.execute("INSERT INTO users VALUES (1, 'Alice')")
    conn.execute("INSERT INTO users VALUES (2, 'Bob')")
    conn.execute("CREATE VIEW user_names AS SELECT name FROM users")
    conn.execute("CREATE INDEX idx_users_name ON users (name)")
    conn.commit()
    conn.close()
    return db


@pytest.fixture()
def modified_db(tmp_path: Path) -> Path:
    """Create a modified database for diff testing."""
    db = tmp_path / "modified.sqlite"
    conn = sqlite3.connect(str(db))
    conn.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)"
    )
    conn.execute("INSERT INTO users VALUES (1, 'Alice', 'alice@example.com')")
    conn.execute("INSERT INTO users VALUES (3, 'Charlie', 'charlie@example.com')")
    conn.execute("CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT)")
    conn.commit()
    conn.close()
    return db


# ---------------------------------------------------------------------------
# Index
# ---------------------------------------------------------------------------


def test_index(client: TestClient) -> None:
    resp = client.get("/")
    assert resp.status_code == 200
    assert "patchworks" in resp.text


# ---------------------------------------------------------------------------
# Schema browser
# ---------------------------------------------------------------------------


def test_browse_form(client: TestClient) -> None:
    resp = client.get("/browse")
    assert resp.status_code == 200
    assert "Schema Browser" in resp.text


def test_browse_schema(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/schema?db={sample_db}")
    assert resp.status_code == 200
    assert "users" in resp.text


def test_browse_schema_missing_db(client: TestClient) -> None:
    resp = client.get("/browse/schema?db=/nonexistent/db.sqlite")
    assert resp.status_code == 200
    assert "Cannot open" in resp.text


def test_browse_schema_no_path(client: TestClient) -> None:
    resp = client.get("/browse/schema")
    assert resp.status_code == 200
    assert "No database path" in resp.text


def test_browse_ddl_table(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/ddl?db={sample_db}&name=users&kind=table")
    assert resp.status_code == 200
    assert "CREATE TABLE" in resp.text


def test_browse_ddl_view(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/ddl?db={sample_db}&name=user_names&kind=view")
    assert resp.status_code == 200
    assert "CREATE VIEW" in resp.text


def test_browse_ddl_index(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/ddl?db={sample_db}&name=idx_users_name&kind=index")
    assert resp.status_code == 200
    assert "CREATE INDEX" in resp.text


# ---------------------------------------------------------------------------
# Row browser
# ---------------------------------------------------------------------------


def test_browse_rows(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/rows?db={sample_db}&table=users")
    assert resp.status_code == 200
    assert "Alice" in resp.text
    assert "Bob" in resp.text


def test_browse_rows_missing_table(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/browse/rows?db={sample_db}&table=nonexistent")
    assert resp.status_code == 200
    assert "Table not found" in resp.text


def test_browse_rows_pagination(client: TestClient, tmp_path: Path) -> None:
    """Verify pagination works with many rows."""
    db = tmp_path / "big.sqlite"
    conn = sqlite3.connect(str(db))
    conn.execute("CREATE TABLE nums (id INTEGER PRIMARY KEY)")
    for i in range(120):
        conn.execute("INSERT INTO nums VALUES (?)", (i,))
    conn.commit()
    conn.close()

    # Page 1
    resp = client.get(f"/browse/rows?db={db}&table=nums&page=1")
    assert resp.status_code == 200
    assert "Page 1" in resp.text

    # Page 3
    resp = client.get(f"/browse/rows?db={db}&table=nums&page=3")
    assert resp.status_code == 200
    assert "Page 3" in resp.text


# ---------------------------------------------------------------------------
# Diff viewer
# ---------------------------------------------------------------------------


def test_diff_form(client: TestClient) -> None:
    resp = client.get("/diff")
    assert resp.status_code == 200
    assert "Diff Viewer" in resp.text


def test_diff_result(client: TestClient, sample_db: Path, modified_db: Path) -> None:
    resp = client.get(f"/diff/result?left={sample_db}&right={modified_db}")
    assert resp.status_code == 200
    # Should show diff results
    assert "users" in resp.text


def test_diff_no_changes(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/diff/result?left={sample_db}&right={sample_db}")
    assert resp.status_code == 200
    assert "No differences" in resp.text


def test_diff_missing_db(client: TestClient, sample_db: Path) -> None:
    resp = client.get(f"/diff/result?left={sample_db}&right=/no/such/db")
    assert resp.status_code == 200
    assert "not found" in resp.text


def test_diff_no_paths(client: TestClient) -> None:
    resp = client.get("/diff/result")
    assert resp.status_code == 200
    assert "required" in resp.text


# ---------------------------------------------------------------------------
# SQL export
# ---------------------------------------------------------------------------


def test_export_form(client: TestClient) -> None:
    resp = client.get("/export")
    assert resp.status_code == 200
    assert "SQL Export" in resp.text


def test_export_preview(client: TestClient, sample_db: Path, modified_db: Path) -> None:
    resp = client.get(f"/export/preview?left={sample_db}&right={modified_db}")
    assert resp.status_code == 200
    assert "Migration SQL" in resp.text


def test_export_download(
    client: TestClient, sample_db: Path, modified_db: Path
) -> None:
    resp = client.get(f"/export/download?left={sample_db}&right={modified_db}")
    assert resp.status_code == 200
    assert "PRAGMA" in resp.text
    assert resp.headers["content-type"].startswith("application/sql")


def test_export_preview_no_paths(client: TestClient) -> None:
    resp = client.get("/export/preview")
    assert resp.status_code == 200
    assert "required" in resp.text


# ---------------------------------------------------------------------------
# Snapshots
# ---------------------------------------------------------------------------


def test_snapshots_page(client: TestClient) -> None:
    resp = client.get("/snapshots")
    assert resp.status_code == 200
    assert "Snapshot" in resp.text


def test_snapshot_save_and_list(
    client: TestClient, sample_db: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Save a snapshot via the web UI and verify it appears in the list."""
    # Use a temp dir for snapshot store to avoid polluting ~/.patchworks
    store_dir = tmp_path / "pw_store"
    monkeypatch.setattr(
        "patchworks.web.routes.SnapshotStore",
        lambda: SnapshotStore(base_dir=store_dir),
    )

    # Save
    resp = client.post(
        "/snapshots/save",
        data={"database": str(sample_db), "name": "test-snap"},
    )
    assert resp.status_code == 200
    assert "test-snap" in resp.text

    # List
    resp = client.get("/snapshots/list")
    assert resp.status_code == 200
    assert "test-snap" in resp.text


def test_snapshot_delete(
    client: TestClient, sample_db: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store_dir = tmp_path / "pw_store"
    store = SnapshotStore(base_dir=store_dir)
    monkeypatch.setattr(
        "patchworks.web.routes.SnapshotStore",
        lambda: store,
    )

    info = store.save(sample_db, name="to-delete")
    resp = client.delete(f"/snapshots/{info.id}")
    assert resp.status_code == 200
    # Should not contain the deleted snapshot
    assert "to-delete" not in resp.text


def test_snapshot_save_missing_db(
    client: TestClient, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store_dir = tmp_path / "pw_store"
    monkeypatch.setattr(
        "patchworks.web.routes.SnapshotStore",
        lambda: SnapshotStore(base_dir=store_dir),
    )
    resp = client.post(
        "/snapshots/save",
        data={"database": "/nonexistent/db.sqlite", "name": "fail"},
    )
    assert resp.status_code == 200
    assert "not found" in resp.text


# ---------------------------------------------------------------------------
# htmx partial responses
# ---------------------------------------------------------------------------


def test_htmx_schema_partial(client: TestClient, sample_db: Path) -> None:
    """htmx requests should get partial HTML, not the full page."""
    resp = client.get(
        f"/browse/schema?db={sample_db}",
        headers={"HX-Request": "true"},
    )
    assert resp.status_code == 200
    # Partial should not contain the full <html> wrapper
    assert "<!DOCTYPE html>" not in resp.text
    assert "users" in resp.text


def test_htmx_diff_partial(
    client: TestClient, sample_db: Path, modified_db: Path
) -> None:
    resp = client.get(
        f"/diff/result?left={sample_db}&right={modified_db}",
        headers={"HX-Request": "true"},
    )
    assert resp.status_code == 200
    assert "<!DOCTYPE html>" not in resp.text


def test_htmx_rows_partial(client: TestClient, sample_db: Path) -> None:
    resp = client.get(
        f"/browse/rows?db={sample_db}&table=users",
        headers={"HX-Request": "true"},
    )
    assert resp.status_code == 200
    assert "<!DOCTYPE html>" not in resp.text
    assert "Alice" in resp.text
