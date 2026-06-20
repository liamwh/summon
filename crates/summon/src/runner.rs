//! Shared hot-path execution for direct mode and daemon mode.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app::{self, AppTarget};
use crate::config::{self, Config, EffectiveSettings};
use crate::controller::{self, AppAction, DecisionContext, MacAppController};

/// Rendered command output for a summon invocation.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct RunOutput {
    /// Whether the command succeeded.
    pub success: bool,
    /// Whether retrying in direct mode may succeed when daemon mode could not.
    #[serde(default)]
    pub should_fallback_direct: bool,
    /// Content to write to stdout.
    pub stdout: String,
    /// Content to write to stderr.
    pub stderr: String,
}

impl RunOutput {
    /// Writes the output to the current process stdio streams.
    pub fn emit(&self) {
        if !self.stdout.is_empty() {
            print!("{}", self.stdout);
        }
        if !self.stderr.is_empty() {
            eprint!("{}", self.stderr);
        }
    }
}

/// Runs `summon app <app>`.
pub fn run_app(app: &str, verbose: u8) -> RunOutput {
    let target = match app::classify_app_target(app) {
        Ok(target) => target,
        Err(err) => {
            return failure(format!("Invalid app target: {err}\n"));
        }
    };

    let settings = config::EffectiveSettings {
        launch_if_not_running: true,
        ..config::EffectiveSettings::default()
    };

    run_target(app, &target, &settings, verbose)
}

/// Runs `summon <binding>` using an explicit config path.
pub fn run_binding_from_path(name: &str, path: &Path, verbose: u8) -> RunOutput {
    let config = match config::load_from(path) {
        Ok(config) => config,
        Err(err) => {
            return failure(format!("Config error in {}:\n  {err}\n", path.display()));
        }
    };

    run_binding_with_config(name, path, &config, verbose)
}

/// Runs `summon <binding>` using a preloaded config.
pub fn run_binding_with_config(name: &str, path: &Path, config: &Config, verbose: u8) -> RunOutput {
    let resolved = match config::resolve_binding(config, name, path) {
        Ok(resolved) => resolved,
        Err(err) => {
            return failure(format!("{err}\n"));
        }
    };

    run_target(
        &resolved.name,
        &resolved.target,
        &resolved.settings,
        verbose,
    )
}

fn run_target(
    label: &str,
    target: &AppTarget,
    settings: &EffectiveSettings,
    verbose: u8,
) -> RunOutput {
    controller::with_autorelease_pool(|| {
        let controller = MacAppController::new();
        let observation = match controller.observe_target(target) {
            Ok(observation) => observation,
            Err(err) => {
                return failure(format!("Failed to inspect {label}: {err}\n"));
            }
        };

        let (action, context) = controller::decide_action_for_observation(observation, settings);
        let stderr = render_decision(verbose, label, target, action, context);

        match controller.execute_action_with_observation(target, action, observation) {
            Ok(()) => RunOutput {
                success: true,
                should_fallback_direct: false,
                stdout: String::new(),
                stderr,
            },
            Err(err) => RunOutput {
                success: false,
                should_fallback_direct: matches!(
                    err,
                    controller::ControllerError::PermissionDenied { .. }
                ),
                stdout: String::new(),
                stderr: format!("{stderr}Failed to {action:?} {label}: {err}\n"),
            },
        }
    })
}

fn failure(stderr: String) -> RunOutput {
    RunOutput {
        success: false,
        should_fallback_direct: false,
        stdout: String::new(),
        stderr,
    }
}

fn render_decision(
    verbose: u8,
    label: &str,
    target: &AppTarget,
    action: AppAction,
    context: DecisionContext,
) -> String {
    if verbose == 0 {
        return String::new();
    }

    let mut output = format!(
        "summon {label}: running={} frontmost={:?} launch={} cycle={} -> {action:?}\n",
        context.is_running,
        context.frontmost,
        context.launch_when_missing,
        context.cycle_when_focused
    );

    if verbose > 1 {
        output.push_str(&format!(
            "  target={}\n",
            controller::target_display(target)
        ));
    }

    output
}
