//! Summon — a tiny macOS command-line tool for opening, focusing,
//! and cycling applications from declarative keybindings.

pub mod app;
mod cli;
pub mod config;

use clap::Parser;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    cli::run(cli)
}
