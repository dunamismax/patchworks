"""CLI subcommand dispatch for patchworks.

Uses :mod:`argparse` from the standard library.  This module is a thin
dispatch layer — no business logic lives here.
"""

from __future__ import annotations

import argparse
import sys
from typing import NoReturn

# ---------------------------------------------------------------------------
# Subcommand handlers (stubs)
# ---------------------------------------------------------------------------


def _cmd_inspect(args: argparse.Namespace) -> int:
    """Inspect a SQLite database."""
    _ = args
    print("inspect: not yet implemented")
    return 0


def _cmd_diff(args: argparse.Namespace) -> int:
    """Diff two SQLite databases."""
    _ = args
    print("diff: not yet implemented")
    return 0


def _cmd_export(args: argparse.Namespace) -> int:
    """Export a SQL migration between two databases."""
    _ = args
    print("export: not yet implemented")
    return 0


def _cmd_snapshot(args: argparse.Namespace) -> int:
    """Manage database snapshots."""
    _ = args
    print("snapshot: not yet implemented")
    return 0


def _cmd_merge(args: argparse.Namespace) -> int:
    """Three-way merge of SQLite databases."""
    _ = args
    print("merge: not yet implemented")
    return 0


def _cmd_migrate(args: argparse.Namespace) -> int:
    """Migration workflow management."""
    _ = args
    print("migrate: not yet implemented")
    return 0


def _cmd_serve(args: argparse.Namespace) -> int:
    """Launch the local web UI."""
    _ = args
    print("serve: not yet implemented")
    return 0


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
        sys.exit(0)

    raise SystemExit(args.func(args))
