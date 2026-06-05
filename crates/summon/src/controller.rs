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
// MacAppController
// ---------------------------------------------------------------------------

/// A macOS app controller that uses `open` and `osascript` to manage apps.
///
/// This controller uses the macOS `open` command for launching and focusing
/// apps, and AppleScript via `osascript` for querying app state (running,
/// frontmost) and cycling windows through the macOS Accessibility API.
pub struct MacAppController;

impl MacAppController {
    /// Creates a new macOS app controller.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Executes an AppleScript expression and returns its trimmed stdout.
    fn run_applescript(script: &str) -> Result<String, String> {
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("AppleScript error: {}", stderr.trim()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Returns `true` if a process with the given bundle identifier is running.
    fn is_bundle_running(&self, bundle_id: &str) -> bool {
        let script = format!(
            "tell application \"System Events\" to (bundle identifier of every process) contains \"{}\"",
            bundle_id
        );
        Self::run_applescript(&script)
            .ok()
            .is_some_and(|s| s == "true")
    }

    /// Returns `true` if a process with the given name is running.
    fn is_process_running(&self, name: &str) -> bool {
        let script = format!(
            "tell application \"System Events\" to (name of every process) contains \"{}\"",
            name
        );
        Self::run_applescript(&script)
            .ok()
            .is_some_and(|s| s == "true")
    }

    /// Returns the bundle identifier of the frontmost application, if available.
    fn frontmost_bundle_id(&self) -> Option<String> {
        let script = "tell application \"System Events\" to get bundle identifier of first process whose frontmost is true";
        Self::run_applescript(script).ok()
    }

    /// Returns the process name of the frontmost application, if available.
    fn frontmost_process_name(&self) -> Option<String> {
        let script = "tell application \"System Events\" to get name of first process whose frontmost is true";
        Self::run_applescript(script).ok()
    }

    /// Extracts the application name from a file path.
    ///
    /// For example, `/Applications/Safari.app` returns `Some("Safari")`.
    /// Returns `None` if the path does not end with `.app`.
    fn app_name_from_path(path: &str) -> Option<&str> {
        let p = std::path::Path::new(path);
        if p.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        {
            p.file_stem().and_then(|s| s.to_str())
        } else {
            None
        }
    }

    /// Builds an AppleScript process reference for the given [`AppTarget`].
    ///
    /// For bundle identifiers, uses `first process whose bundle identifier is`
    /// to look up the process. For app names and paths, uses `process "Name"`.
    fn process_ref_script(target: &AppTarget) -> String {
        match target {
            AppTarget::BundleId(id) => {
                format!("first process whose bundle identifier is \"{id}\"")
            }
            AppTarget::AppName(name) => {
                format!("process \"{name}\"")
            }
            AppTarget::AppPath(path) => {
                let name = Self::app_name_from_path(path).unwrap_or(path);
                format!("process \"{name}\"")
            }
        }
    }
}

impl Default for MacAppController {
    fn default() -> Self {
        Self::new()
    }
}

impl AppController for MacAppController {
    fn is_running(&self, target: &AppTarget) -> bool {
        match target {
            AppTarget::BundleId(id) => self.is_bundle_running(id),
            AppTarget::AppName(name) => self.is_process_running(name),
            AppTarget::AppPath(path) => {
                Self::app_name_from_path(path).is_some_and(|name| self.is_process_running(name))
            }
        }
    }

    fn is_frontmost(&self, target: &AppTarget) -> bool {
        match target {
            AppTarget::BundleId(id) => self.frontmost_bundle_id().is_some_and(|f| f == *id),
            AppTarget::AppName(name) => self.frontmost_process_name().is_some_and(|f| f == *name),
            AppTarget::AppPath(path) => Self::app_name_from_path(path)
                .is_some_and(|name| self.frontmost_process_name().is_some_and(|f| f == name)),
        }
    }

    fn launch(&self, target: &AppTarget) -> Result<(), String> {
        let result = match target {
            AppTarget::BundleId(id) => std::process::Command::new("open")
                .args(["-b", id])
                .output()
                .map_err(|e| format!("Failed to run open: {e}"))?,
            AppTarget::AppName(name) => std::process::Command::new("open")
                .args(["-a", name])
                .output()
                .map_err(|e| format!("Failed to run open: {e}"))?,
            AppTarget::AppPath(path) => std::process::Command::new("open")
                .arg(path)
                .output()
                .map_err(|e| format!("Failed to run open: {e}"))?,
        };

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format_app_error(target, stderr.trim()));
        }

        Ok(())
    }

    fn focus(&self, target: &AppTarget) -> Result<(), String> {
        // `open` brings the app to the foreground whether it was just launched
        // or was already running, so launch and focus share the same mechanism.
        self.launch(target)
    }

    fn cycle_window(&self, target: &AppTarget) -> Result<(), String> {
        let process_ref = Self::process_ref_script(target);
        let script = format!(
            "tell application \"System Events\"\n\
             \ttell {process_ref}\n\
             \t\tif (count of windows) >= 2 then\n\
             \t\t\tset index of window 2 to 1\n\
             \t\tend if\n\
             \tend tell\n\
             end tell"
        );

        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("Failed to run osascript for window cycling: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format_cycle_error(stderr.trim()));
        }

        Ok(())
    }
}

/// Formats an error message from the `open` command with context about the target.
fn format_app_error(target: &AppTarget, stderr: &str) -> String {
    match target {
        AppTarget::BundleId(id) => {
            format!("Could not open app with bundle identifier \"{id}\": {stderr}")
        }
        AppTarget::AppName(name) => {
            format!("Could not open app \"{name}\": {stderr}")
        }
        AppTarget::AppPath(path) => {
            format!("Could not open app at \"{path}\": {stderr}")
        }
    }
}

/// Formats an error from the window cycling AppleScript.
fn format_cycle_error(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("not allowed") || lower.contains("not authorized") {
        "Summon needs Accessibility permission to cycle windows.\n\
         Open:\n\
         \tSystem Settings \u{2192} Privacy & Security \u{2192} Accessibility\n\
         Then enable the terminal, launcher, or hotkey daemon that invokes Summon."
            .to_string()
    } else {
        format!("Could not cycle windows: {stderr}")
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

    // -- MacAppController construction -----------------------------------------

    #[test]
    fn mac_controller_new_works() {
        let _controller = super::MacAppController::new();
    }

    #[test]
    fn mac_controller_default_works() {
        let _controller: super::MacAppController = Default::default();
    }

    // -- app_name_from_path ---------------------------------------------------

    #[test]
    fn app_name_from_path_standard() {
        assert_eq!(
            super::MacAppController::app_name_from_path("/Applications/Safari.app"),
            Some("Safari")
        );
    }

    #[test]
    fn app_name_from_path_with_spaces() {
        assert_eq!(
            super::MacAppController::app_name_from_path("/Applications/Visual Studio Code.app"),
            Some("Visual Studio Code")
        );
    }

    #[test]
    fn app_name_from_path_nested() {
        assert_eq!(
            super::MacAppController::app_name_from_path("/Applications/Utilities/Terminal.app"),
            Some("Terminal")
        );
    }

    #[test]
    fn app_name_from_path_tilde() {
        assert_eq!(
            super::MacAppController::app_name_from_path("~/Applications/My App.app"),
            Some("My App")
        );
    }

    #[test]
    fn app_name_from_path_no_app_extension() {
        assert_eq!(
            super::MacAppController::app_name_from_path("/Applications/Safari"),
            None
        );
    }

    #[test]
    fn app_name_from_path_empty() {
        assert_eq!(super::MacAppController::app_name_from_path(""), None);
    }

    // -- format_app_error -----------------------------------------------------

    #[test]
    fn format_app_error_bundle_id() {
        let target = AppTarget::BundleId("com.example.app".into());
        let msg = super::format_app_error(&target, "not found");
        assert!(msg.contains("com.example.app"), "should contain bundle ID");
        assert!(msg.contains("not found"), "should contain stderr");
    }

    #[test]
    fn format_app_error_app_name() {
        let target = AppTarget::AppName("Safari".into());
        let msg = super::format_app_error(&target, "unable to find");
        assert!(msg.contains("Safari"), "should contain app name");
        assert!(msg.contains("unable to find"), "should contain stderr");
    }

    #[test]
    fn format_app_error_app_path() {
        let target = AppTarget::AppPath("/Apps/Test.app".into());
        let msg = super::format_app_error(&target, "does not exist");
        assert!(msg.contains("/Apps/Test.app"), "should contain path");
        assert!(msg.contains("does not exist"), "should contain stderr");
    }

    // -- MacAppController cycle_window ----------------------------------------

    // This test calls real macOS Accessibility APIs and can take ~60 seconds
    // when Accessibility permissions are not granted (AppleScript timeout).
    // Run manually with: cargo test -p summon --lib -- --ignored mac_controller_cycle
    #[test]
    #[ignore]
    fn mac_controller_cycle_runs_without_panic() {
        // Finder is always running on macOS. The cycle may succeed, fail with
        // an accessibility error, or fail with a System Events connection error
        // depending on the test environment. We verify only that the method
        // runs without panicking and returns a well-typed result.
        let controller = super::MacAppController::new();
        let target = finder();
        let _result = controller.cycle_window(&target);
    }

    // -- process_ref_script ----------------------------------------------------

    #[test]
    fn process_ref_bundle_id() {
        let target = AppTarget::BundleId("com.apple.finder".into());
        let script = super::MacAppController::process_ref_script(&target);
        assert_eq!(
            script,
            "first process whose bundle identifier is \"com.apple.finder\""
        );
    }

    #[test]
    fn process_ref_app_name() {
        let target = AppTarget::AppName("Preview".into());
        let script = super::MacAppController::process_ref_script(&target);
        assert_eq!(script, "process \"Preview\"");
    }

    #[test]
    fn process_ref_app_path() {
        let target = AppTarget::AppPath("/Applications/Safari.app".into());
        let script = super::MacAppController::process_ref_script(&target);
        assert_eq!(script, "process \"Safari\"");
    }

    #[test]
    fn process_ref_app_path_with_spaces() {
        let target = AppTarget::AppPath("/Applications/Visual Studio Code.app".into());
        let script = super::MacAppController::process_ref_script(&target);
        assert_eq!(script, "process \"Visual Studio Code\"");
    }

    // -- format_cycle_error ----------------------------------------------------

    #[test]
    fn format_cycle_error_accessibility_denied() {
        // macOS returns "Not allowed" with capital N.
        let msg = super::format_cycle_error(
            "System Events got an error: Not allowed to send keystrokes.",
        );
        assert!(
            msg.contains("Accessibility permission"),
            "should mention accessibility: {msg}"
        );
        assert!(
            msg.contains("Privacy & Security"),
            "should mention settings path: {msg}"
        );
    }

    #[test]
    fn format_cycle_error_not_authorized() {
        let msg = super::format_cycle_error("osascript is Not authorized to do stuff");
        assert!(
            msg.contains("Accessibility permission"),
            "should mention accessibility: {msg}"
        );
    }

    #[test]
    fn format_cycle_error_generic() {
        let msg = super::format_cycle_error("Some unknown error");
        assert!(
            msg.contains("Could not cycle windows"),
            "should mention cycling: {msg}"
        );
        assert!(
            msg.contains("Some unknown error"),
            "should include original error: {msg}"
        );
    }
}
