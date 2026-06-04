//! Command-line interface for Summon.

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
#[derive(Debug, clap::Subcommand)]
pub enum ConfigCommand {
    /// Print the active configuration file path.
    Path,

    /// Validate the configuration file and print any errors.
    Check,
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
