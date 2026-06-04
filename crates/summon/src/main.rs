//! Summon — a tiny macOS command-line tool for opening, focusing,
//! and cycling applications from declarative keybindings.

mod cli;

use clap::Parser;

fn main() {
    let _cli = cli::Cli::parse();
}
