//! Command-line interface for Summon.

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use summon::app;
use summon::config;
use summon::controller;
use summon::diagnostics;

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
        Some(Command::App { ref app }) => run_app(app),
        Some(Command::List) => run_list(),
        Some(Command::Doctor) => run_doctor(),
        None => {
            if let Some(binding) = cli.binding {
                run_binding(&binding)
            } else {
                // No args — print help so new users see usage information.
                let _ = Cli::command().print_help();
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

/// `summon list` — prints all configured bindings.
fn run_list() -> ExitCode {
    let path = match config::config_path() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let config = match config::load_from(&path) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Config error in {}:", path.display());
            eprintln!("  {err}");
            return ExitCode::FAILURE;
        }
    };

    if config.bindings.is_empty() {
        println!("No bindings configured.");
        return ExitCode::SUCCESS;
    }

    let max_name_len = config.bindings.keys().map(String::len).max().unwrap_or(0);

    for (name, binding) in &config.bindings {
        println!("{name:max_name_len$} -> {}", binding.app);
    }

    ExitCode::SUCCESS
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

/// `summon app <app>` — summon an app directly by name, bundle ID, or path.
///
/// Classifies the app string, uses sensible defaults (launch if not running,
/// no cycling), and runs the decide/execute cycle.
fn run_app(app: &str) -> ExitCode {
    let target = match app::classify_app_target(app) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("Invalid app target: {err}");
            return ExitCode::FAILURE;
        }
    };

    let settings = config::EffectiveSettings {
        launch_if_not_running: true,
        ..config::EffectiveSettings::default()
    };

    let ctrl = controller::MacAppController::new();
    let action = controller::decide_action(&ctrl, &target, &settings);

    if let Err(err) = controller::execute_action(&ctrl, &target, action) {
        eprintln!("Failed to {action:?} {app}: {err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// `summon <binding>` — the core command path.
///
/// Loads config, resolves the binding, decides the action, and executes it.
fn run_binding(name: &str) -> ExitCode {
    let path = match config::config_path() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let config = match config::load_from(&path) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Config error in {}:", path.display());
            eprintln!("  {err}");
            return ExitCode::FAILURE;
        }
    };

    let resolved = match config::resolve_binding(&config, name, &path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let controller = controller::MacAppController::new();

    let action = controller::decide_action(&controller, &resolved.target, &resolved.settings);

    if let Err(err) = controller::execute_action(&controller, &resolved.target, action) {
        eprintln!("Failed to {action:?} {}: {err}", resolved.name);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

/// `summon doctor` — runs diagnostic checks.
fn run_doctor() -> ExitCode {
    println!("Summon doctor");
    println!();
    let result = diagnostics::run_doctor();
    println!();
    println!(
        "{} check(s): {} passed, {} warning(s), {} failed",
        result.checks, result.passed, result.warnings, result.failures
    );

    if result.is_ok() {
        println!("Summon looks healthy.");
        ExitCode::SUCCESS
    } else {
        eprintln!("Some checks failed. See above for details.");
        ExitCode::FAILURE
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
    fn no_args_parses_successfully_and_run_prints_help() {
        // Parsing succeeds (no required args), but run() prints help.
        let cli = Cli::try_parse_from(["summon"]).expect("should parse");
        assert!(cli.binding.is_none());
        assert!(cli.command.is_none());
    }

    // -- List formatting tests -----------------------------------------------

    #[test]
    fn format_binding_list_aligns_names() {
        let config = config::parse(
            r#"
            [bindings.browser]
            app = "com.brave.Browser"

            [bindings.terminal]
            app = "com.mitchellh.ghostty"

            [bindings.editor]
            app = "dev.zed.Zed"
            "#,
        )
        .expect("should parse");

        let max_name_len = config.bindings.keys().map(String::len).max().unwrap_or(0);

        // "terminal" is the longest name at 8 chars
        assert_eq!(max_name_len, 8);

        let lines: Vec<String> = config
            .bindings
            .iter()
            .map(|(name, binding)| format!("{name:max_name_len$} -> {}", binding.app))
            .collect();

        // BTreeMap gives sorted order: browser, editor, terminal
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "browser  -> com.brave.Browser");
        assert_eq!(lines[1], "editor   -> dev.zed.Zed");
        assert_eq!(lines[2], "terminal -> com.mitchellh.ghostty");
    }

    #[test]
    fn format_binding_list_single_binding() {
        let config = config::parse(
            r#"
            [bindings.finder]
            app = "com.apple.finder"
            "#,
        )
        .expect("should parse");

        let max_name_len = config.bindings.keys().map(String::len).max().unwrap_or(0);

        let lines: Vec<String> = config
            .bindings
            .iter()
            .map(|(name, binding)| format!("{name:max_name_len$} -> {}", binding.app))
            .collect();

        assert_eq!(lines, ["finder -> com.apple.finder"]);
    }

    #[test]
    fn format_binding_list_empty_config() {
        let config = config::parse("").expect("should parse empty config");
        assert!(config.bindings.is_empty());
    }
}
