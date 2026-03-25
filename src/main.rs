use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use patchworks::app::{PatchworksApp, StartupOptions};
use patchworks::cli::{self, OutputFormat};
use patchworks::db::snapshot::SnapshotStore;

/// Patchworks — Git-style diffs for SQLite databases.
#[derive(Debug, Parser)]
#[command(
    name = "patchworks",
    about = "Git-style diffs for SQLite databases.",
    version,
    after_help = "Run without a subcommand to launch the desktop GUI."
)]
struct Cli {
    /// Create a snapshot for the given database and exit (legacy shorthand for `snapshot save`).
    #[arg(long, hide = true)]
    snapshot: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,

    /// Zero, one, or two database files to open in the GUI.
    files: Vec<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect a SQLite database: show tables, columns, views, indexes, and triggers.
    Inspect {
        /// Path to the SQLite database.
        database: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },

    /// Diff two SQLite databases and show schema and data changes.
    ///
    /// Exit code 0 means no differences. Exit code 2 means differences were found.
    Diff {
        /// Left (source/before) database path.
        left: PathBuf,
        /// Right (target/after) database path.
        right: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },

    /// Generate a SQL migration that transforms the left database into the right database.
    Export {
        /// Left (source/before) database path.
        left: PathBuf,
        /// Right (target/after) database path.
        right: PathBuf,
        /// Write the migration SQL to a file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Three-way merge: compare two databases against a common ancestor.
    ///
    /// Detects conflicts and shows which changes can be auto-merged.
    Merge {
        /// Ancestor (common base) database path.
        ancestor: PathBuf,
        /// Left (first derived) database path.
        left: PathBuf,
        /// Right (second derived) database path.
        right: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },

    /// Manage database snapshots.
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },
}

#[derive(Debug, Subcommand)]
enum SnapshotAction {
    /// Save a new snapshot of a database.
    Save {
        /// Path to the database to snapshot.
        database: PathBuf,
        /// Human-readable name for the snapshot.
        #[arg(long)]
        name: Option<String>,
    },

    /// List saved snapshots.
    List {
        /// Filter to snapshots from a specific source database.
        #[arg(long)]
        source: Option<PathBuf>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },

    /// Delete a saved snapshot.
    Delete {
        /// Snapshot UUID to delete.
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum Format {
    /// Human-readable text output.
    Human,
    /// Machine-readable JSON output.
    Json,
}

impl From<Format> for OutputFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Human => OutputFormat::Human,
            Format::Json => OutputFormat::Json,
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Normalize single-dash `-snapshot` to `--snapshot` for backward compatibility.
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

    // Legacy `--snapshot <db>` path — kept for backward compatibility.
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

    // Subcommand dispatch.
    if let Some(command) = cli.command {
        let mut stdout = std::io::stdout().lock();
        let exit_code = match command {
            Command::Inspect { database, format } => {
                cli::run_inspect(&mut stdout, &database, format.into()).context("inspect failed")?
            }
            Command::Diff {
                left,
                right,
                format,
            } => cli::run_diff(&mut stdout, &left, &right, format.into()).context("diff failed")?,
            Command::Merge {
                ancestor,
                left,
                right,
                format,
            } => cli::run_merge(&mut stdout, &ancestor, &left, &right, format.into())
                .context("merge failed")?,
            Command::Export {
                left,
                right,
                output,
            } => {
                if let Some(output_path) = output {
                    let file = std::fs::File::create(&output_path)
                        .with_context(|| format!("create output file {}", output_path.display()))?;
                    let mut writer = std::io::BufWriter::new(file);
                    cli::run_export(&mut writer, &left, &right).context("export failed")?
                } else {
                    cli::run_export(&mut stdout, &left, &right).context("export failed")?
                }
            }
            Command::Snapshot { action } => match action {
                SnapshotAction::Save { database, name } => {
                    cli::run_snapshot_save(&mut stdout, &database, name.as_deref())
                        .context("snapshot save failed")?
                }
                SnapshotAction::List { source, format } => {
                    cli::run_snapshot_list(&mut stdout, source.as_deref(), format.into())
                        .context("snapshot list failed")?
                }
                SnapshotAction::Delete { id } => {
                    cli::run_snapshot_delete(&mut stdout, &id).context("snapshot delete failed")?
                }
            },
        };
        std::process::exit(exit_code);
    }

    // Default: launch the desktop GUI.
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
