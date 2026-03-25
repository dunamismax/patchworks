"""Smoke tests for the patchworks CLI."""

from __future__ import annotations

import subprocess
import sys

import patchworks


def test_version_string() -> None:
    assert patchworks.__version__ == "0.1.0"


def test_help_exits_zero() -> None:
    """``patchworks --help`` exits 0."""
    result = subprocess.run(
        [sys.executable, "-m", "patchworks", "--help"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert "patchworks" in result.stdout


def test_help_shows_all_subcommands() -> None:
    """The help text lists every top-level subcommand."""
    result = subprocess.run(
        [sys.executable, "-m", "patchworks", "--help"],
        capture_output=True,
        text=True,
    )
    for cmd in (
        "inspect",
        "diff",
        "export",
        "snapshot",
        "merge",
        "migrate",
        "serve",
    ):
        assert cmd in result.stdout, f"missing subcommand: {cmd}"


def test_version_flag() -> None:
    """``patchworks --version`` prints the version."""
    result = subprocess.run(
        [sys.executable, "-m", "patchworks", "--version"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert "0.1.0" in result.stdout


def test_inspect_stub() -> None:
    """``patchworks inspect`` stub returns 0."""
    result = subprocess.run(
        [sys.executable, "-m", "patchworks", "inspect", "/dev/null"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert "not yet implemented" in result.stdout


def test_no_args_shows_help() -> None:
    """Running with no arguments shows help and exits 0."""
    result = subprocess.run(
        [sys.executable, "-m", "patchworks"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0
    assert "patchworks" in result.stdout
