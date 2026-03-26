"""Comparison algorithms, export generation, and semantic analysis."""

from __future__ import annotations

from patchworks.diff.schema import diff_schemas

__all__ = [
    "diff_schemas",
]


def __getattr__(name: str) -> object:
    # Lazy imports to break circular dependency chains between db/ and diff/.
    if name == "diff_table_data":
        from patchworks.diff.data import diff_table_data

        return diff_table_data
    if name in ("export_as_sql", "write_export"):
        from patchworks.diff import export as _export

        return getattr(_export, name)
    if name in ("analyze", "filter_diff", "summarize_diff"):
        from patchworks.diff import semantic as _semantic

        return getattr(_semantic, name)
    msg = f"module {__name__!r} has no attribute {name!r}"
    raise AttributeError(msg)
