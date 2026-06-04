//! Command-line interface for Summon.

use std::process::ExitCode;

use crate::config;
use clap::Parser;

/// Summon — open, focus, and cycle macOS apps from your keyboard.
#[derive(Debug, Parser)]
#[command(name = "summon", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// A binding name to summon directly (shorthand for `summon binding <name>`).
    #[arg(value_name = "BINDING")]
    pub binding: Option<String>,
}

/// Summon subcommands.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Summon an app directly by name or bundle identifier.
    App {
        /// Application name, bundle identifier, or path.
        app: String,
    },

    /// List all configured bindings.
    List,

    /// Show or validate the configuration file.
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommand,
    },

    /// Check whether Summon has the macOS permissions it needs.
    Doctor,
}

/// Configuration subcommands.
#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum ConfigCommand {
    /// Print the active configuration file path.
    Path,

    /// Validate the configuration file and print any errors.
    Check,
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Runs the parsed CLI command.
///
/// Returns [`ExitCode::SUCCESS`] on success, [`ExitCode::FAILURE`] on error.
/// Errors are printed to stderr.
pub fn run(cli: Cli) -> ExitCode {
    match cli.command {
        Some(Command::Config { subcommand }) => run_config(subcommand),
        Some(Command::App { .. }) => {
            eprintln!("not yet implemented: summon app");
            ExitCode::FAILURE
        }
        Some(Command::List) => {
            eprintln!("not yet implemented: summon list");
            ExitCode::FAILURE
        }
        Some(Command::Doctor) => {
            eprintln!("not yet implemented: summon doctor");
            ExitCode::FAILURE
        }
        None => {
            if let Some(binding) = cli.binding {
                eprintln!("not yet implemented: summon <binding> (binding: {binding})");
                ExitCode::FAILURE
            } else {
                // No args — clap already printed help.
                ExitCode::SUCCESS
            }
        }
    }
}

/// Dispatches `summon config` subcommands.
fn run_config(subcommand: ConfigCommand) -> ExitCode {
    match subcommand {
        ConfigCommand::Path => run_config_path(),
        ConfigCommand::Check => run_config_check(),
    }
}

/// `summon config path` — prints the active config file path.
fn run_config_path() -> ExitCode {
    match config::config_path() {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

/// `summon config check` — validates the config file.
fn run_config_check() -> ExitCode {
    let path = match config::config_path() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    match config::load_from(&path) {
        Ok(config) => {
            let count = config.bindings.len();
            println!("Config is valid: {}", path.display());
            println!("  {count} binding(s) configured");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Config error in {}:", path.display());
            eprintln!("  {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_binding_shorthand() {
        let cli = Cli::try_parse_from(["summon", "terminal"]).expect("should parse");
        assert_eq!(cli.binding.as_deref(), Some("terminal"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn parse_app_subcommand() {
        let cli =
            Cli::try_parse_from(["summon", "app", "com.mitchellh.ghostty"]).expect("should parse");
        match cli.command {
            Some(Command::App { app }) => assert_eq!(app, "com.mitchellh.ghostty"),
            other => panic!("expected App command, got {other:?}"),
        }
    }

    #[test]
    fn parse_list() {
        let cli = Cli::try_parse_from(["summon", "list"]).expect("should parse");
        assert!(matches!(cli.command, Some(Command::List)));
    }

    #[test]
    fn parse_config_path() {
        let cli = Cli::try_parse_from(["summon", "config", "path"]).expect("should parse");
        match cli.command {
            Some(Command::Config {
                subcommand: ConfigCommand::Path,
            }) => {}
            other => panic!("expected Config Path, got {other:?}"),
        }
    }

    #[test]
    fn parse_config_check() {
        let cli = Cli::try_parse_from(["summon", "config", "check"]).expect("should parse");
        match cli.command {
            Some(Command::Config {
                subcommand: ConfigCommand::Check,
            }) => {}
            other => panic!("expected Config Check, got {other:?}"),
        }
    }

    #[test]
    fn parse_doctor() {
        let cli = Cli::try_parse_from(["summon", "doctor"]).expect("should parse");
        assert!(matches!(cli.command, Some(Command::Doctor)));
    }

    #[test]
    fn positional_arg_accepted_as_binding() {
        let cli = Cli::try_parse_from(["summon", "explode"]).expect("should parse as binding");
        assert_eq!(cli.binding.as_deref(), Some("explode"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn reject_no_args_when_binding_expected_context() {
        // No args is valid — it just prints help.
        let cli = Cli::try_parse_from(["summon"]).expect("should parse");
        assert!(cli.binding.is_none());
        assert!(cli.command.is_none());
    }
}
