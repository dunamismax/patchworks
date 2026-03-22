# BUILD.md

Short build and verification notes for Patchworks.

## Commands

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
cargo run -- --help
cargo run -- --snapshot app.db
cargo run -- app.db
cargo run -- left.db right.db
```

## Notes

- `src/db` owns inspection and snapshots
- `src/diff` owns schema diff, row diff, and SQL export
- best results come from stable SQLite files, not heavily changing live databases
