//! App controller trait and decision logic for Summon.
//!
//! The [`AppController`] trait defines the narrow boundary between Summon's
//! decision logic and macOS application management. A [`FakeAppController`]
//! enables fully deterministic testing of launch/focus/cycle behaviour without
//! macOS Accessibility permissions or a running GUI.
//!
//! # Decision logic
//!
//! [`decide_action`] is a pure function that determines what to do with a target
//! app based on its current state and the effective settings:
//!
//! 1. If the app is not running and `launch_if_not_running`, return [`AppAction::Launch`].
//! 2. If the app is running but not frontmost, return [`AppAction::Focus`].
//! 3. If the app is frontmost and `cycle_when_focused`, return [`AppAction::Cycle`].
//! 4. If the app is frontmost and cycling is disabled, return [`AppAction::NoOp`].
//! 5. If the app is not running and `launch_if_not_running` is false, return
//!    [`AppAction::NoOp`] (nothing to do).

use crate::app::AppTarget;
use crate::config::EffectiveSettings;

// ---------------------------------------------------------------------------
// AppAction
// ---------------------------------------------------------------------------

/// The action Summon should take for a target application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// Launch the application (it is not running).
    Launch,
    /// Focus the application (it is running but not frontmost).
    Focus,
    /// Cycle to the next window (the app is already frontmost).
    Cycle,
    /// Do nothing (the app is frontmost and cycling is disabled, or the app is
    /// not running and `launch_if_not_running` is false).
    NoOp,
}

// ---------------------------------------------------------------------------
// AppController trait
// ---------------------------------------------------------------------------

/// Interface for querying and controlling macOS applications.
///
/// This trait separates Summon's decision logic from operating-system side
/// effects. The real implementation will call macOS APIs; the fake
/// implementation enables deterministic testing.
pub trait AppController {
    /// Returns `true` if the target application is currently running.
    fn is_running(&self, target: &AppTarget) -> bool;

    /// Returns `true` if the target application is currently the frontmost app.
    fn is_frontmost(&self, target: &AppTarget) -> bool;

    /// Launches the target application.
    ///
    /// # Errors
    ///
    /// Implementations should return a descriptive error if the app cannot be
    /// launched (not found, permission denied, etc.).
    fn launch(&self, target: &AppTarget) -> Result<(), String>;

    /// Brings the target application to the foreground.
    ///
    /// # Errors
    ///
    /// Implementations should return a descriptive error if the app cannot be
    /// focused (not running, Accessibility permission missing, etc.).
    fn focus(&self, target: &AppTarget) -> Result<(), String>;

    /// Cycles to the next window belonging to the target application.
    ///
    /// # Errors
    ///
    /// Implementations should return a descriptive error if cycling fails.
    fn cycle_window(&self, target: &AppTarget) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Decision logic
// ---------------------------------------------------------------------------

/// Determines the appropriate action for a target application.
///
/// This is a pure function: it has no side effects and depends only on the
/// controller's view of app state and the resolved settings.
///
/// # Decision table
///
/// | Running | Frontmost | Launch? | Cycle? | Action   |
/// |---------|-----------|---------|--------|----------|
/// | No      | -         | Yes     | -      | Launch   |
/// | No      | -         | No      | -      | NoOp     |
/// | Yes     | No        | -       | -      | Focus    |
/// | Yes     | Yes       | -       | Yes    | Cycle    |
/// | Yes     | Yes       | -       | No     | NoOp     |
pub fn decide_action(
    controller: &dyn AppController,
    target: &AppTarget,
    settings: &EffectiveSettings,
) -> AppAction {
    if !controller.is_running(target) {
        if settings.launch_if_not_running {
            AppAction::Launch
        } else {
            AppAction::NoOp
        }
    } else if !controller.is_frontmost(target) {
        AppAction::Focus
    } else if settings.cycle_when_focused {
        AppAction::Cycle
    } else {
        AppAction::NoOp
    }
}

/// Executes the decided action against the controller.
///
/// # Errors
///
/// Returns the controller's error string if the action fails.
pub fn execute_action(
    controller: &dyn AppController,
    target: &AppTarget,
    action: AppAction,
) -> Result<(), String> {
    match action {
        AppAction::Launch => controller.launch(target),
        AppAction::Focus => controller.focus(target),
        AppAction::Cycle => controller.cycle_window(target),
        AppAction::NoOp => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// FakeAppController
// ---------------------------------------------------------------------------

/// A fake app controller for deterministic testing.
///
/// Use the builder-like methods to configure app state before exercising the
/// decision logic.
///
/// # Example
///
/// ```
/// use summon::controller::{decide_action, FakeAppController, AppAction};
/// use summon::app::AppTarget;
/// use summon::config::EffectiveSettings;
///
/// let target = AppTarget::BundleId("com.apple.finder".into());
/// let settings = EffectiveSettings::default();
///
/// let controller = FakeAppController::new()
///     .set_running(&target, false)
///     .set_frontmost(&target, false);
///
/// let action = decide_action(&controller, &target, &settings);
/// assert_eq!(action, AppAction::NoOp); // launch_if_not_running defaults to false
/// ```
#[derive(Clone, Debug, Default)]
pub struct FakeAppController {
    running: Vec<AppTarget>,
    frontmost: Vec<AppTarget>,
}

impl FakeAppController {
    /// Creates a new fake controller with no apps running or frontmost.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the given target as running (or not).
    #[must_use]
    pub fn set_running(mut self, target: &AppTarget, running: bool) -> Self {
        if running {
            if !self.running.contains(target) {
                self.running.push(target.clone());
            }
        } else {
            self.running.retain(|t| t != target);
        }
        self
    }

    /// Marks the given target as frontmost (or not).
    #[must_use]
    pub fn set_frontmost(mut self, target: &AppTarget, frontmost: bool) -> Self {
        if frontmost {
            if !self.frontmost.contains(target) {
                self.frontmost.push(target.clone());
            }
        } else {
            self.frontmost.retain(|t| t != target);
        }
        self
    }
}

impl AppController for FakeAppController {
    fn is_running(&self, target: &AppTarget) -> bool {
        self.running.contains(target)
    }

    fn is_frontmost(&self, target: &AppTarget) -> bool {
        self.frontmost.contains(target)
    }

    fn launch(&self, _target: &AppTarget) -> Result<(), String> {
        Ok(())
    }

    fn focus(&self, _target: &AppTarget) -> Result<(), String> {
        Ok(())
    }

    fn cycle_window(&self, _target: &AppTarget) -> Result<(), String> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Helper: bundle ID target for tests.
    fn finder() -> AppTarget {
        AppTarget::BundleId("com.apple.finder".into())
    }

    /// Helper: settings with launch enabled, cycling disabled.
    fn launch_settings() -> EffectiveSettings {
        EffectiveSettings {
            launch_if_not_running: true,
            cycle_when_focused: false,
            ..EffectiveSettings::default()
        }
    }

    /// Helper: settings with cycling enabled, launch enabled.
    fn cycle_settings() -> EffectiveSettings {
        EffectiveSettings {
            launch_if_not_running: true,
            cycle_when_focused: true,
            ..EffectiveSettings::default()
        }
    }

    // -- decide_action: not running -------------------------------------------

    #[test]
    fn not_running_with_launch_returns_launch() {
        let target = finder();
        let controller = FakeAppController::new();
        let action = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Launch);
    }

    #[test]
    fn not_running_without_launch_returns_noop() {
        let target = finder();
        let controller = FakeAppController::new();
        let settings = EffectiveSettings {
            launch_if_not_running: false,
            ..EffectiveSettings::default()
        };
        let action = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::NoOp);
    }

    // -- decide_action: running but not frontmost ----------------------------

    #[test]
    fn running_not_frontmost_returns_focus() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        let action = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Focus);
    }

    #[test]
    fn running_not_frontmost_ignores_cycle_setting() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        // Even with cycling enabled, a non-frontmost running app should focus.
        let action = decide_action(&controller, &target, &cycle_settings());
        assert_eq!(action, AppAction::Focus);
    }

    // -- decide_action: running and frontmost ---------------------------------

    #[test]
    fn frontmost_with_cycle_returns_cycle() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let action = decide_action(&controller, &target, &cycle_settings());
        assert_eq!(action, AppAction::Cycle);
    }

    #[test]
    fn frontmost_without_cycle_returns_noop() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let action = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::NoOp);
    }

    // -- decide_action: all defaults (nothing enabled) ------------------------

    #[test]
    fn all_defaults_not_running_is_noop() {
        let target = finder();
        let controller = FakeAppController::new();
        let settings = EffectiveSettings::default();
        let action = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::NoOp);
    }

    #[test]
    fn all_defaults_running_not_frontmost_is_focus() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        let settings = EffectiveSettings::default();
        let action = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::Focus);
    }

    #[test]
    fn all_defaults_frontmost_is_noop() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let settings = EffectiveSettings::default();
        let action = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::NoOp);
    }

    // -- decide_action: different target types --------------------------------

    #[test]
    fn decide_with_app_name_target() {
        let target = AppTarget::AppName("Preview".into());
        let controller = FakeAppController::new();
        let action = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Launch);
    }

    #[test]
    fn decide_with_app_path_target() {
        let target = AppTarget::AppPath("/Applications/My App.app".into());
        let controller = FakeAppController::new().set_running(&target, true);
        let action = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Focus);
    }

    // -- execute_action -------------------------------------------------------

    #[test]
    fn execute_noop_succeeds() {
        let target = finder();
        let controller = FakeAppController::new();
        let result = execute_action(&controller, &target, AppAction::NoOp);
        assert!(result.is_ok());
    }

    #[test]
    fn execute_launch_succeeds_with_fake() {
        let target = finder();
        let controller = FakeAppController::new();
        let result = execute_action(&controller, &target, AppAction::Launch);
        assert!(result.is_ok());
    }

    #[test]
    fn execute_focus_succeeds_with_fake() {
        let target = finder();
        let controller = FakeAppController::new();
        let result = execute_action(&controller, &target, AppAction::Focus);
        assert!(result.is_ok());
    }

    #[test]
    fn execute_cycle_succeeds_with_fake() {
        let target = finder();
        let controller = FakeAppController::new();
        let result = execute_action(&controller, &target, AppAction::Cycle);
        assert!(result.is_ok());
    }

    // -- FakeAppController builder -------------------------------------------

    #[test]
    fn fake_controller_default_is_empty() {
        let controller = FakeAppController::new();
        let target = finder();
        assert!(!controller.is_running(&target));
        assert!(!controller.is_frontmost(&target));
    }

    #[test]
    fn fake_controller_set_running() {
        let target = finder();
        let controller = FakeAppController::new().set_running(&target, true);
        assert!(controller.is_running(&target));

        let controller = controller.set_running(&target, false);
        assert!(!controller.is_running(&target));
    }

    #[test]
    fn fake_controller_set_frontmost() {
        let target = finder();
        let controller = FakeAppController::new().set_frontmost(&target, true);
        assert!(controller.is_frontmost(&target));

        let controller = controller.set_frontmost(&target, false);
        assert!(!controller.is_frontmost(&target));
    }

    #[test]
    fn fake_controller_multiple_targets() {
        let finder = AppTarget::BundleId("com.apple.finder".into());
        let zed = AppTarget::BundleId("dev.zed.Zed".into());

        let controller = FakeAppController::new()
            .set_running(&finder, true)
            .set_running(&zed, true)
            .set_frontmost(&finder, true);

        assert!(controller.is_running(&finder));
        assert!(controller.is_running(&zed));
        assert!(controller.is_frontmost(&finder));
        assert!(!controller.is_frontmost(&zed));
    }

    #[test]
    fn fake_controller_idempotent_set() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_running(&target, true);
        assert!(controller.is_running(&target));
    }
}
