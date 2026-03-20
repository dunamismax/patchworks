use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use patchworks::app::{PatchworksApp, StartupOptions};
use patchworks::db::snapshot::SnapshotStore;

/// Patchworks command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "patchworks",
    about = "Git-style visual diffs for SQLite databases."
)]
struct Cli {
    /// Create a snapshot for the given database and exit.
    #[arg(long)]
    snapshot: Option<PathBuf>,
    /// Zero, one, or two database files to open in the UI.
    files: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = std::env::args()
        .map(|arg| {
            if arg == "-snapshot" {
                "--snapshot".to_owned()
            } else {
                arg
            }
        })
        .collect::<Vec<_>>();
    let cli = Cli::parse_from(args);

    if let Some(path) = cli.snapshot {
        let store = SnapshotStore::new_default().context("create snapshot store")?;
        let name = format!(
            "{} snapshot",
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("database")
        );
        let snapshot = store
            .save_snapshot(&path, &name)
            .with_context(|| format!("save snapshot for {}", path.display()))?;
        println!("Saved snapshot {} ({})", snapshot.name, snapshot.id);
        return Ok(());
    }

    let startup = match cli.files.as_slice() {
        [] => StartupOptions::default(),
        [one] => StartupOptions {
            left: Some(one.clone()),
            right: None,
        },
        [left, right] => StartupOptions {
            left: Some(left.clone()),
            right: Some(right.clone()),
        },
        _ => anyhow::bail!("expected at most two database file arguments"),
    };

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Patchworks",
        native_options,
        Box::new(move |_creation_context| {
            let app = PatchworksApp::new(startup.clone())
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    Ok(())
}
