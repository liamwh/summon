//! Summon binary entry point.

use clap::Parser;

mod cli;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    cli::run(cli)
}
