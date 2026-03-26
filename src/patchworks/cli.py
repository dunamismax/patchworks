"""CLI subcommand dispatch for patchworks.

Uses :mod:`argparse` from the standard library.  This module is a thin
dispatch layer — no business logic lives here.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import TYPE_CHECKING, Any, NoReturn

if TYPE_CHECKING:
    from patchworks.db.types import (
        DatabaseDiff,
        DatabaseSummary,
        DiffSummary,
        SemanticAnalysis,
        TableDataDiff,
    )
    from patchworks.diff.merge import MergeResult

# ---------------------------------------------------------------------------
# Exit codes
# ---------------------------------------------------------------------------

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_DIFFERENCES = 2

# ---------------------------------------------------------------------------
# Subcommand handlers
# ---------------------------------------------------------------------------


def _cmd_inspect(args: argparse.Namespace) -> int:
    """Inspect a SQLite database."""
    from pathlib import Path

    from patchworks.db.inspector import inspect_database

    db_path = Path(args.database)
    if not db_path.exists():
        print(f"error: database not found: {db_path}", file=sys.stderr)
        return EXIT_ERROR

    try:
        summary = inspect_database(db_path)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    if args.format == "json":
        print(json.dumps(_summary_to_dict(summary), indent=2))
    else:
        _print_summary_human(summary)
    return EXIT_OK


def _cmd_diff(args: argparse.Namespace) -> int:
    """Diff two SQLite databases."""
    from pathlib import Path

    from patchworks.db.differ import diff_databases
    from patchworks.diff.semantic import analyze, filter_diff, summarize_diff

    left = Path(args.left)
    right = Path(args.right)

    for p, label in ((left, "left"), (right, "right")):
        if not p.exists():
            print(f"error: {label} database not found: {p}", file=sys.stderr)
            return EXIT_ERROR

    try:
        result = diff_databases(left, right)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    # Apply filters if specified.
    change_types: set[str] | None = None
    filter_tables: set[str] | None = None
    if hasattr(args, "change_type") and args.change_type:
        change_types = set(args.change_type)
    if hasattr(args, "table") and args.table:
        filter_tables = set(args.table)
    if change_types is not None or filter_tables is not None:
        result = filter_diff(result, change_types=change_types, tables=filter_tables)

    has_changes = result.has_changes
    show_summary = getattr(args, "summary", False)
    show_semantic = getattr(args, "semantic", False)

    if args.format == "json":
        d = _diff_to_dict(result)
        if show_summary:
            d["summary"] = _summary_to_stats_dict(summarize_diff(result))
        if show_semantic:
            d["semantic"] = _semantic_to_dict(analyze(result))
        print(json.dumps(d, indent=2))
    else:
        _print_diff_human(result)
        if show_summary:
            _print_summary_stats(summarize_diff(result))
        if show_semantic:
            _print_semantic(analyze(result))

    return EXIT_DIFFERENCES if has_changes else EXIT_OK


def _cmd_export(args: argparse.Namespace) -> int:
    """Export a SQL migration between two databases."""
    from pathlib import Path

    from patchworks.db.differ import diff_databases
    from patchworks.diff.export import write_export

    left = Path(args.left)
    right = Path(args.right)

    for p, label in ((left, "left"), (right, "right")):
        if not p.exists():
            print(f"error: {label} database not found: {p}", file=sys.stderr)
            return EXIT_ERROR

    try:
        result = diff_databases(left, right)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    if args.output:
        try:
            with open(args.output, "w") as f:
                write_export(result, f, right_path=right)
        except OSError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return EXIT_ERROR
    else:
        write_export(result, sys.stdout, right_path=right)  # type: ignore[arg-type]

    return EXIT_OK


def _cmd_snapshot(args: argparse.Namespace) -> int:
    """Manage database snapshots."""
    sub = getattr(args, "snapshot_command", None)
    if not sub:
        print(
            "error: specify a snapshot subcommand: save, list, delete",
            file=sys.stderr,
        )
        return EXIT_ERROR

    if sub == "save":
        return _cmd_snapshot_save(args)
    elif sub == "list":
        return _cmd_snapshot_list(args)
    elif sub == "delete":
        return _cmd_snapshot_delete(args)
    else:
        print(f"error: unknown snapshot command: {sub}", file=sys.stderr)
        return EXIT_ERROR


def _cmd_snapshot_save(args: argparse.Namespace) -> int:
    from pathlib import Path

    from patchworks.db.snapshot import SnapshotStore

    db_path = Path(args.database)
    if not db_path.exists():
        print(f"error: database not found: {db_path}", file=sys.stderr)
        return EXIT_ERROR

    try:
        store = SnapshotStore()
        info = store.save(db_path, name=getattr(args, "name", None))
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    print(f"Snapshot saved: {info.id}")
    if info.name:
        print(f"  Name: {info.name}")
    print(f"  Source: {info.source}")
    print(f"  Size: {info.size_bytes} bytes")
    print(f"  Created: {info.created_at}")
    return EXIT_OK


def _cmd_snapshot_list(args: argparse.Namespace) -> int:
    from patchworks.db.snapshot import SnapshotStore

    try:
        store = SnapshotStore()
        snapshots = store.list(source=getattr(args, "source", None))
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    fmt = getattr(args, "format", "human")

    if fmt == "json":
        data = [
            {
                "id": s.id,
                "source": s.source,
                "name": s.name,
                "created_at": s.created_at,
                "file_path": s.file_path,
                "size_bytes": s.size_bytes,
            }
            for s in snapshots
        ]
        print(json.dumps(data, indent=2))
    else:
        if not snapshots:
            print("No snapshots found.")
        else:
            for s in snapshots:
                label = f" ({s.name})" if s.name else ""
                print(f"{s.id}{label}")
                print(f"  Source: {s.source}")
                print(f"  Size: {s.size_bytes} bytes")
                print(f"  Created: {s.created_at}")
                print()

    return EXIT_OK


def _cmd_snapshot_delete(args: argparse.Namespace) -> int:
    from patchworks.db.snapshot import SnapshotStore

    try:
        store = SnapshotStore()
        deleted = store.delete(args.uuid)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    if deleted:
        print(f"Snapshot deleted: {args.uuid}")
        return EXIT_OK
    else:
        print(f"error: snapshot not found: {args.uuid}", file=sys.stderr)
        return EXIT_ERROR


def _cmd_merge(args: argparse.Namespace) -> int:
    """Three-way merge of SQLite databases."""
    from pathlib import Path

    from patchworks.diff.merge import merge_databases

    ancestor = Path(args.ancestor)
    left = Path(args.left)
    right = Path(args.right)

    for p, label in ((ancestor, "ancestor"), (left, "left"), (right, "right")):
        if not p.exists():
            print(f"error: {label} database not found: {p}", file=sys.stderr)
            return EXIT_ERROR

    try:
        result = merge_databases(str(ancestor), str(left), str(right))
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return EXIT_ERROR

    if args.format == "json":
        print(json.dumps(_merge_to_dict(result), indent=2))
    else:
        _print_merge_human(result)

    return EXIT_DIFFERENCES if result.has_conflicts else EXIT_OK


def _cmd_migrate(args: argparse.Namespace) -> int:
    """Migration workflow management."""
    _ = args
    print("migrate: not yet implemented")
    return EXIT_OK


def _cmd_serve(args: argparse.Namespace) -> int:
    """Launch the local web UI."""
    _ = args
    print("serve: not yet implemented")
    return EXIT_OK


# ---------------------------------------------------------------------------
# Parser construction
# ---------------------------------------------------------------------------


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="patchworks",
        description="Git-style diffs for SQLite databases.",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"%(prog)s {_get_version()}",
    )

    sub = parser.add_subparsers(dest="command", title="commands")

    # inspect ---------------------------------------------------------------
    p_inspect = sub.add_parser("inspect", help="Inspect a SQLite database")
    p_inspect.add_argument("database", help="Path to the SQLite database")
    p_inspect.add_argument(
        "--format",
        choices=["human", "json"],
        default="human",
        help="Output format (default: human)",
    )
    p_inspect.set_defaults(func=_cmd_inspect)

    # diff ------------------------------------------------------------------
    p_diff = sub.add_parser("diff", help="Diff two SQLite databases")
    p_diff.add_argument("left", help="Path to the left (base) database")
    p_diff.add_argument("right", help="Path to the right (target) database")
    p_diff.add_argument(
        "--format",
        choices=["human", "json"],
        default="human",
        help="Output format (default: human)",
    )
    p_diff.add_argument(
        "--change-type",
        action="append",
        choices=["added", "removed", "modified"],
        help="Filter by change type (can be repeated)",
    )
    p_diff.add_argument(
        "--table",
        action="append",
        help="Filter to specific table(s) (can be repeated)",
    )
    p_diff.add_argument(
        "--summary",
        action="store_true",
        help="Show aggregate diff summary statistics",
    )
    p_diff.add_argument(
        "--semantic",
        action="store_true",
        help="Show semantic analysis (renames, type shifts)",
    )
    p_diff.set_defaults(func=_cmd_diff)

    # export ----------------------------------------------------------------
    p_export = sub.add_parser(
        "export", help="Generate SQL migration between two databases"
    )
    p_export.add_argument("left", help="Path to the left (base) database")
    p_export.add_argument("right", help="Path to the right (target) database")
    p_export.add_argument(
        "-o",
        "--output",
        help="Write SQL to file instead of stdout",
    )
    p_export.set_defaults(func=_cmd_export)

    # snapshot --------------------------------------------------------------
    p_snapshot = sub.add_parser("snapshot", help="Manage database snapshots")
    snap_sub = p_snapshot.add_subparsers(
        dest="snapshot_command", title="snapshot commands"
    )

    p_snap_save = snap_sub.add_parser("save", help="Save a snapshot")
    p_snap_save.add_argument("database", help="Path to the database to snapshot")
    p_snap_save.add_argument("--name", help="Label for the snapshot")

    p_snap_list = snap_sub.add_parser("list", help="List snapshots")
    p_snap_list.add_argument("--source", help="Filter by source database path")
    p_snap_list.add_argument(
        "--format",
        choices=["human", "json"],
        default="human",
        help="Output format (default: human)",
    )

    p_snap_del = snap_sub.add_parser("delete", help="Delete a snapshot")
    p_snap_del.add_argument("uuid", help="Snapshot UUID to delete")

    p_snapshot.set_defaults(func=_cmd_snapshot)

    # merge -----------------------------------------------------------------
    p_merge = sub.add_parser("merge", help="Three-way merge of SQLite databases")
    p_merge.add_argument("ancestor", help="Path to the common-ancestor database")
    p_merge.add_argument("left", help="Path to the left database")
    p_merge.add_argument("right", help="Path to the right database")
    p_merge.add_argument(
        "--format",
        choices=["human", "json"],
        default="human",
        help="Output format (default: human)",
    )
    p_merge.set_defaults(func=_cmd_merge)

    # migrate ---------------------------------------------------------------
    p_migrate = sub.add_parser("migrate", help="Migration workflow management")
    mig_sub = p_migrate.add_subparsers(dest="migrate_command", title="migrate commands")

    p_mig_gen = mig_sub.add_parser("generate", help="Generate a migration")
    p_mig_gen.add_argument("left", help="Base database")
    p_mig_gen.add_argument("right", help="Target database")
    p_mig_gen.add_argument("--name", help="Migration name")
    p_mig_gen.add_argument("--dry-run", action="store_true", help="Preview only")

    p_mig_validate = mig_sub.add_parser("validate", help="Validate a migration")
    p_mig_validate.add_argument("id", help="Migration ID")

    p_mig_list = mig_sub.add_parser("list", help="List migrations")
    p_mig_list.add_argument(
        "--format",
        choices=["human", "json"],
        default="human",
        help="Output format",
    )

    p_mig_show = mig_sub.add_parser("show", help="Show migration details")
    p_mig_show.add_argument("id", help="Migration ID")

    p_mig_apply = mig_sub.add_parser("apply", help="Apply a migration")
    p_mig_apply.add_argument("id", help="Migration ID")
    p_mig_apply.add_argument("target", help="Target database")
    p_mig_apply.add_argument("--dry-run", action="store_true", help="Preview only")

    p_mig_delete = mig_sub.add_parser("delete", help="Delete a migration")
    p_mig_delete.add_argument("id", help="Migration ID")

    p_mig_squash = mig_sub.add_parser("squash", help="Squash migrations")
    p_mig_squash.add_argument("--source", help="Source database")
    p_mig_squash.add_argument("--name", help="Squashed migration name")
    p_mig_squash.add_argument("--dry-run", action="store_true", help="Preview only")

    mig_sub.add_parser("conflicts", help="Show migration conflicts")

    p_migrate.set_defaults(func=_cmd_migrate)

    # serve -----------------------------------------------------------------
    p_serve = sub.add_parser("serve", help="Launch local web UI")
    p_serve.add_argument(
        "--port",
        type=int,
        default=8000,
        help="Port to listen on (default: 8000)",
    )
    p_serve.set_defaults(func=_cmd_serve)

    return parser


# ---------------------------------------------------------------------------
# Formatting helpers
# ---------------------------------------------------------------------------


def _summary_to_dict(summary: DatabaseSummary) -> dict[str, object]:
    """Convert a :class:`DatabaseSummary` to a JSON-friendly dict."""
    return {
        "path": summary.path,
        "page_size": summary.page_size,
        "page_count": summary.page_count,
        "journal_mode": summary.journal_mode,
        "tables": [
            {
                "name": t.name,
                "columns": [
                    {
                        "name": c.name,
                        "type": c.type,
                        "notnull": c.notnull,
                        "default_value": c.default_value,
                        "primary_key": c.primary_key,
                    }
                    for c in t.columns
                ],
                "primary_key_columns": list(t.primary_key_columns),
                "without_rowid": t.without_rowid,
                "row_count": t.row_count,
                "indexes": [
                    {
                        "name": i.name,
                        "table_name": i.table_name,
                        "unique": i.unique,
                        "columns": list(i.columns),
                        "partial": i.partial,
                        "sql": i.sql,
                    }
                    for i in t.indexes
                ],
                "triggers": [
                    {"name": tr.name, "table_name": tr.table_name, "sql": tr.sql}
                    for tr in t.triggers
                ],
                "sql": t.sql,
            }
            for t in summary.tables
        ],
        "views": [
            {
                "name": v.name,
                "columns": [
                    {
                        "name": c.name,
                        "type": c.type,
                        "notnull": c.notnull,
                        "default_value": c.default_value,
                        "primary_key": c.primary_key,
                    }
                    for c in v.columns
                ],
                "sql": v.sql,
            }
            for v in summary.views
        ],
    }


def _print_summary_human(summary: DatabaseSummary) -> None:
    """Print a human-readable summary to stdout."""
    print(f"Database: {summary.path}")
    print(f"Page size: {summary.page_size}")
    print(f"Pages: {summary.page_count}")
    print(f"Journal mode: {summary.journal_mode}")
    print(f"Tables: {len(summary.tables)}")
    print(f"Views: {len(summary.views)}")
    print(f"Indexes: {len(summary.indexes)}")
    print(f"Triggers: {len(summary.triggers)}")

    for table in summary.tables:
        print(f"\n  Table: {table.name} ({table.row_count} rows)")
        if table.without_rowid:
            print("    WITHOUT ROWID")
        if table.primary_key_columns:
            print(f"    PK: {', '.join(table.primary_key_columns)}")
        for col in table.columns:
            parts = [f"    {col.name} {col.type}".rstrip()]
            if col.notnull:
                parts.append("NOT NULL")
            if col.default_value is not None:
                parts.append(f"DEFAULT {col.default_value}")
            if col.primary_key:
                parts.append(f"PK({col.primary_key})")
            print(" ".join(parts))
        for idx in table.indexes:
            uniq = " UNIQUE" if idx.unique else ""
            print(f"    Index: {idx.name}{uniq} ({', '.join(idx.columns)})")
        for trigger in table.triggers:
            print(f"    Trigger: {trigger.name}")

    for view in summary.views:
        print(f"\n  View: {view.name}")
        for col in view.columns:
            print(f"    {col.name} {col.type}".rstrip())


def _diff_to_dict(diff: DatabaseDiff) -> dict[str, Any]:
    """Convert a :class:`DatabaseDiff` to a JSON-friendly dict."""
    sd = diff.schema_diff
    return {
        "left_path": diff.left_path,
        "right_path": diff.right_path,
        "has_changes": diff.has_changes,
        "schema": {
            "tables_added": [t.name for t in sd.tables_added],
            "tables_removed": [t.name for t in sd.tables_removed],
            "tables_modified": [
                {
                    "name": tm.table_name,
                    "columns_added": [c.name for c in tm.columns_added],
                    "columns_removed": [c.name for c in tm.columns_removed],
                    "columns_modified": [
                        {"name": old.name, "old_type": old.type, "new_type": new.type}
                        for old, new in tm.columns_modified
                    ],
                }
                for tm in sd.tables_modified
            ],
            "indexes_added": [i.name for i in sd.indexes_added],
            "indexes_removed": [i.name for i in sd.indexes_removed],
            "indexes_modified": [i.name for i in sd.indexes_modified],
            "triggers_added": [t.name for t in sd.triggers_added],
            "triggers_removed": [t.name for t in sd.triggers_removed],
            "triggers_modified": [t.name for t in sd.triggers_modified],
            "views_added": [v.name for v in sd.views_added],
            "views_removed": [v.name for v in sd.views_removed],
            "views_modified": [v.name for v in sd.views_modified],
        },
        "data": [_table_data_diff_to_dict(td) for td in diff.table_data_diffs],
        "warnings": list(diff.warnings),
    }


def _table_data_diff_to_dict(td: TableDataDiff) -> dict[str, Any]:
    """Convert a :class:`TableDataDiff` to a JSON-friendly dict."""
    return {
        "table_name": td.table_name,
        "key_columns": list(td.key_columns),
        "rows_added": td.rows_added,
        "rows_removed": td.rows_removed,
        "rows_modified": td.rows_modified,
        "row_diffs": [
            {
                "kind": rd.kind,
                "key": list(rd.key),
                "old_values": _serialize_row(rd.old_values),
                "new_values": _serialize_row(rd.new_values),
                "cell_changes": [
                    {
                        "column": cc.column,
                        "old_value": _serialize_value(cc.old_value),
                        "new_value": _serialize_value(cc.new_value),
                    }
                    for cc in rd.cell_changes
                ],
            }
            for rd in td.row_diffs
        ],
        "warnings": list(td.warnings),
    }


def _serialize_row(row: dict[str, Any] | None) -> dict[str, Any] | None:
    """Serialize a row dict, converting bytes to hex strings."""
    if row is None:
        return None
    return {k: _serialize_value(v) for k, v in row.items()}


def _serialize_value(val: Any) -> Any:
    """Serialize a value for JSON output."""
    if isinstance(val, bytes):
        return val.hex()
    return val


def _print_diff_human(diff: DatabaseDiff) -> None:
    """Print a human-readable diff to stdout."""
    sd = diff.schema_diff

    if not diff.has_changes:
        print("No differences found.")
        return

    # Schema changes
    if sd.has_changes:
        print("Schema changes:")
        for t in sd.tables_added:
            print(f"  + Table: {t.name}")
        for t in sd.tables_removed:
            print(f"  - Table: {t.name}")
        for tm in sd.tables_modified:
            print(f"  ~ Table: {tm.table_name}")
            for c in tm.columns_added:
                print(f"      + Column: {c.name} {c.type}")
            for c in tm.columns_removed:
                print(f"      - Column: {c.name} {c.type}")
            for old, new in tm.columns_modified:
                print(f"      ~ Column: {old.name} ({old.type} -> {new.type})")
        for i in sd.indexes_added:
            print(f"  + Index: {i.name}")
        for i in sd.indexes_removed:
            print(f"  - Index: {i.name}")
        for im in sd.indexes_modified:
            print(f"  ~ Index: {im.name}")
        for t in sd.triggers_added:
            print(f"  + Trigger: {t.name}")
        for t in sd.triggers_removed:
            print(f"  - Trigger: {t.name}")
        for tm in sd.triggers_modified:
            print(f"  ~ Trigger: {tm.name}")
        for v in sd.views_added:
            print(f"  + View: {v.name}")
        for v in sd.views_removed:
            print(f"  - View: {v.name}")
        for vm in sd.views_modified:
            print(f"  ~ View: {vm.name}")
        print()

    # Data changes
    for td in diff.table_data_diffs:
        if not (td.rows_added or td.rows_removed or td.rows_modified):
            continue
        print(
            f"Table {td.table_name}: "
            f"+{td.rows_added} -{td.rows_removed} ~{td.rows_modified} rows"
        )
        for rd in td.row_diffs:
            key_str = ", ".join(str(v) for v in rd.key)
            if rd.kind == "added":
                print(f"  + [{key_str}]")
            elif rd.kind == "removed":
                print(f"  - [{key_str}]")
            elif rd.kind == "modified":
                changes = ", ".join(
                    f"{cc.column}: {cc.old_value!r} -> {cc.new_value!r}"
                    for cc in rd.cell_changes
                )
                print(f"  ~ [{key_str}] {changes}")

    # Warnings
    if diff.warnings:
        print()
        for w in diff.warnings:
            print(f"warning: {w}")


def _summary_to_stats_dict(summary: DiffSummary) -> dict[str, int]:
    """Convert a :class:`DiffSummary` to a JSON-friendly dict."""
    return {
        "tables_added": summary.tables_added,
        "tables_removed": summary.tables_removed,
        "tables_modified": summary.tables_modified,
        "indexes_added": summary.indexes_added,
        "indexes_removed": summary.indexes_removed,
        "indexes_modified": summary.indexes_modified,
        "triggers_added": summary.triggers_added,
        "triggers_removed": summary.triggers_removed,
        "triggers_modified": summary.triggers_modified,
        "views_added": summary.views_added,
        "views_removed": summary.views_removed,
        "views_modified": summary.views_modified,
        "total_rows_added": summary.total_rows_added,
        "total_rows_removed": summary.total_rows_removed,
        "total_rows_modified": summary.total_rows_modified,
        "total_cell_changes": summary.total_cell_changes,
    }


def _semantic_to_dict(analysis: SemanticAnalysis) -> dict[str, Any]:
    """Convert a :class:`SemanticAnalysis` to a JSON-friendly dict."""
    return {
        "table_renames": [
            {
                "old_name": r.old_name,
                "new_name": r.new_name,
                "confidence": r.confidence,
                "matched_columns": list(r.matched_columns),
            }
            for r in analysis.table_renames
        ],
        "column_renames": [
            {
                "table_name": r.table_name,
                "old_name": r.old_name,
                "new_name": r.new_name,
                "confidence": r.confidence,
            }
            for r in analysis.column_renames
        ],
        "type_shifts": [
            {
                "table_name": ts.table_name,
                "column_name": ts.column_name,
                "old_type": ts.old_type,
                "new_type": ts.new_type,
                "old_affinity": ts.old_affinity,
                "new_affinity": ts.new_affinity,
                "compatible": ts.compatible,
                "confidence": ts.confidence,
            }
            for ts in analysis.type_shifts
        ],
        "annotations": [
            {
                "target": a.target,
                "status": a.status,
                "note": a.note,
            }
            for a in analysis.annotations
        ],
    }


def _print_summary_stats(summary: DiffSummary) -> None:
    """Print aggregate diff summary statistics."""
    ta = summary.tables_added
    tr = summary.tables_removed
    tm = summary.tables_modified
    ia = summary.indexes_added
    ir = summary.indexes_removed
    im = summary.indexes_modified
    ga = summary.triggers_added
    gr = summary.triggers_removed
    gm = summary.triggers_modified
    va = summary.views_added
    vr = summary.views_removed
    vm = summary.views_modified
    ra = summary.total_rows_added
    rr = summary.total_rows_removed
    rm = summary.total_rows_modified
    cc = summary.total_cell_changes

    print("\nSummary:")
    print(f"  Schema: +{ta} -{tr} ~{tm} tables")
    print(f"          +{ia} -{ir} ~{im} indexes")
    print(f"          +{ga} -{gr} ~{gm} triggers")
    print(f"          +{va} -{vr} ~{vm} views")
    print(f"  Data:   +{ra} -{rr} ~{rm} rows")
    print(f"          {cc} cell changes")


def _print_semantic(analysis: SemanticAnalysis) -> None:
    """Print semantic analysis results."""
    if analysis.table_renames:
        print("\nProbable table renames:")
        for r in analysis.table_renames:
            print(
                f"  {r.old_name} -> {r.new_name} "
                f"(confidence: {r.confidence:.0%}, "
                f"matched: {', '.join(r.matched_columns)})"
            )

    if analysis.column_renames:
        print("\nProbable column renames:")
        for r in analysis.column_renames:
            print(
                f"  {r.table_name}.{r.old_name} -> {r.new_name} "
                f"(confidence: {r.confidence:.0%})"
            )

    if analysis.type_shifts:
        print("\nType shifts:")
        for ts in analysis.type_shifts:
            compat = "compatible" if ts.compatible else "INCOMPATIBLE"
            print(
                f"  {ts.table_name}.{ts.column_name}: "
                f"{ts.old_type} -> {ts.new_type} "
                f"({ts.old_affinity} -> {ts.new_affinity}, {compat})"
            )


def _merge_to_dict(result: MergeResult) -> dict[str, Any]:
    """Convert a :class:`MergeResult` to a JSON-friendly dict."""
    return {
        "ancestor_path": result.ancestor_path,
        "left_path": result.left_path,
        "right_path": result.right_path,
        "is_clean": result.is_clean,
        "conflicts": [
            {
                "kind": c.kind,
                "table": c.table,
                "description": c.description,
                "left_detail": c.left_detail,
                "right_detail": c.right_detail,
                "key": list(c.key),
            }
            for c in result.conflicts
        ],
        "merged_rows": [
            {
                "table": mr.table,
                "kind": mr.kind,
                "source": mr.source,
                "key": list(mr.key),
                "values": _serialize_row(mr.values),
            }
            for mr in result.merged_rows
        ],
        "merged_schema": [
            {
                "table": ms.table,
                "kind": ms.kind,
                "source": ms.source,
                "sql": ms.sql,
            }
            for ms in result.merged_schema
        ],
    }


def _print_merge_human(result: MergeResult) -> None:
    """Print a human-readable merge result to stdout."""
    if result.is_clean and not result.merged_rows and not result.merged_schema:
        print("No changes to merge.")
        return

    if result.merged_schema:
        print("Schema changes (merged):")
        for ms in result.merged_schema:
            prefix = {"added": "+", "removed": "-", "modified": "~"}.get(ms.kind, "?")
            print(f"  {prefix} Table: {ms.table} (from {ms.source})")
        print()

    if result.merged_rows:
        # Group by table.
        tables: dict[str, list[Any]] = {}
        for mr in result.merged_rows:
            tables.setdefault(mr.table, []).append(mr)

        print("Row changes (merged):")
        for table_name in sorted(tables):
            rows = tables[table_name]
            added = sum(1 for r in rows if r.kind == "added")
            removed = sum(1 for r in rows if r.kind == "removed")
            modified = sum(1 for r in rows if r.kind == "modified")
            print(f"  Table {table_name}: +{added} -{removed} ~{modified} rows")
        print()

    if result.has_conflicts:
        print(f"CONFLICTS ({len(result.conflicts)}):")
        for c in result.conflicts:
            print(f"  [{c.kind}] {c.description}")
            if c.left_detail:
                print(f"    left:  {c.left_detail}")
            if c.right_detail:
                print(f"    right: {c.right_detail}")
        print()
        print("Merge has conflicts. Manual resolution required.")
    else:
        print("Merge is clean. No conflicts.")


def _get_version() -> str:
    from patchworks import __version__

    return __version__


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> NoReturn:
    """Parse arguments and dispatch to the appropriate subcommand."""
    parser = _build_parser()
    args = parser.parse_args(argv)

    if not args.command:
        parser.print_help()
        sys.exit(EXIT_OK)

    raise SystemExit(args.func(args))
