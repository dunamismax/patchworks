"""Patchworks — Git-style diffs for SQLite databases."""

from __future__ import annotations

__version__ = "0.1.0"


def cli_main() -> None:
    """Entry point for the ``patchworks`` console script."""
    from patchworks.cli import main

    main()
