# Fixtures

**Named SQL fixture blocks for Patchworks integration tests.**

`create_fixtures.sql` contains named fixture blocks used by integration tests.

## Format

- each block starts with `-- @fixture <name>`
- the test harness executes the selected block into a temporary SQLite database during the test run

## Purpose

- keep integration-test databases deterministic
- centralize reusable schema and data setup in one place
