use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Tie files into compact .knot archives, and untie them again.
#[derive(Parser)]
#[command(name = "knot", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Tie a file up: pack it into <file>.knot.
    Tie {
        /// The file to tie.
        file: PathBuf,
    },
    /// Untie a .knot file back into the original.
    Untie {
        /// The .knot file to untie.
        file: PathBuf,
        /// Where to write the restored file (defaults to the original name).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Inspect a .knot file's metadata without untying it.
    Inspect {
        /// The .knot file to inspect.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Tie { file } => knot::tie(&file),
        Command::Untie { file, output } => knot::untie(&file, output.as_deref()),
        Command::Inspect { file } => knot::inspect(&file),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("knot: {err}");
            ExitCode::FAILURE
        }
    }
}
