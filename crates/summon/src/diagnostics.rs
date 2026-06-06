//! Diagnostics for Summon.
//!
//! The [`run_doctor`] function checks the health of the Summon installation,
//! including config readability, binding validity, app target resolution, and
//! macOS Accessibility permissions.

use std::path::{Path, PathBuf};

use crate::app::{self, AppTarget};
use crate::config;
use crate::controller::{
    self, current_process_info, grandparent_process_info, parent_process_info, AppStateProbe,
    MacAppStateProbe,
};

use accessibility::attribute::AXUIElementAttributes;
use accessibility::ui_element::AXUIElement;
use accessibility_sys::{
    kAXTrustedCheckOptionPrompt, AXAPIEnabled, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

// ---------------------------------------------------------------------------
// Doctor result
// ---------------------------------------------------------------------------

/// The result of running `summon doctor`.
///
/// Tracks counts of checks, passes, warnings, and failures.
#[derive(Debug)]
pub struct DoctorResult {
    /// Total number of checks run.
    pub checks: usize,
    /// Number of checks that passed.
    pub passed: usize,
    /// Number of checks that produced warnings.
    pub warnings: usize,
    /// Number of checks that failed.
    pub failures: usize,
}

impl DoctorResult {
    /// Creates a new, empty result.
    #[must_use]
    pub fn new() -> Self {
        Self {
            checks: 0,
            passed: 0,
            warnings: 0,
            failures: 0,
        }
    }

    /// Records a passing check.
    pub fn pass(&mut self) {
        self.checks += 1;
        self.passed += 1;
    }

    /// Records a warning.
    pub fn warn(&mut self) {
        self.checks += 1;
        self.warnings += 1;
    }

    /// Records a failure.
    pub fn fail(&mut self) {
        self.checks += 1;
        self.failures += 1;
    }

    /// Returns `true` if no failures occurred.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.failures == 0
    }
}

impl Default for DoctorResult {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Doctor checks
// ---------------------------------------------------------------------------

/// Options for `summon doctor`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DoctorOptions<'a> {
    /// Request the macOS Accessibility permission prompt if not trusted.
    pub request_accessibility: bool,
    /// Optional app target to inspect.
    pub target: Option<&'a AppTarget>,
}

/// Runs all diagnostic checks and returns the result.
///
/// Checks:
///
/// 1. Config file path can be resolved.
/// 2. Config file exists and is readable.
/// 3. Config TOML is valid.
/// 4. Each binding has a valid app target.
/// 5. App path targets point to existing files (warning if missing).
/// 6. macOS Accessibility permission status.
pub fn run_doctor(options: DoctorOptions<'_>) -> DoctorResult {
    let mut result = DoctorResult::new();

    let config_path = check_config_path(&mut result);

    let config = check_config_file(&config_path, &mut result);

    if let Some(config) = config.as_ref() {
        check_bindings(config, &mut result);
    }

    check_accessibility_permission(options.request_accessibility, &mut result);
    check_accessibility_smoke(options.target, &mut result);

    result
}

/// Resolves and prints the config path.
fn check_config_path(result: &mut DoctorResult) -> Option<PathBuf> {
    match config::config_path() {
        Ok(path) => {
            println!("Config path: {}", path.display());
            result.pass();
            Some(path)
        }
        Err(err) => {
            println!("Config path: could not resolve — {err}");
            result.fail();
            None
        }
    }
}

/// Checks that the config file exists and can be parsed.
///
/// Returns `Some(config)` on success, `None` on failure.
fn check_config_file(path: &Option<PathBuf>, result: &mut DoctorResult) -> Option<config::Config> {
    let path = match path {
        Some(p) => p,
        None => {
            println!("  Config file: skipped (path not resolved)");
            return None;
        }
    };

    if !path.exists() {
        println!("  Config file: not found at {}", path.display());
        println!("    Create one with:");
        println!("      mkdir -p ~/.config/summon");
        println!("      $EDITOR ~/.config/summon/summon.toml");
        result.fail();
        return None;
    }

    match config::load_from(path) {
        Ok(config) => {
            let count = config.bindings.len();
            println!("  Config file: valid ({} binding(s))", count);
            result.pass();
            Some(config)
        }
        Err(err) => {
            println!("  Config file: invalid — {err}");
            result.fail();
            None
        }
    }
}

/// Checks each binding's app target classification.
///
/// For path-based targets, also checks if the file exists on disk.
pub(crate) fn check_bindings(config: &config::Config, result: &mut DoctorResult) {
    if config.bindings.is_empty() {
        println!("  Bindings: none configured");
        return;
    }

    for (name, binding) in &config.bindings {
        match app::classify_app_target(&binding.app) {
            Ok(target) => {
                let target_label = format_target_label(&target);

                if let AppTarget::AppPath(p) = &target {
                    let expanded = if p.starts_with('~') {
                        shellexpand_home(p)
                    } else {
                        p.clone()
                    };

                    if Path::new(&expanded).exists() {
                        println!("  Binding \"{name}\": {target_label} (exists)");
                        result.pass();
                    } else {
                        println!("  Binding \"{name}\": {target_label} (path does not exist)");
                        result.warn();
                    }
                } else {
                    println!("  Binding \"{name}\": {target_label}");
                    result.pass();
                }
            }
            Err(err) => {
                println!("  Binding \"{name}\": invalid — {err}");
                result.fail();
            }
        }
    }
}

/// Formats an [`AppTarget`] as a human-readable label.
fn format_target_label(target: &AppTarget) -> String {
    match target {
        AppTarget::BundleId(id) => format!("bundle identifier \"{id}\""),
        AppTarget::AppName(n) => format!("app name \"{n}\""),
        AppTarget::AppPath(p) => format!("app path \"{p}\""),
    }
}

/// Checks macOS Accessibility permission for the current process.
///
/// Reports a failure only when the permission is definitively denied.
/// Timeouts and other errors are reported as warnings so they don't fail
/// the overall `summon doctor` run in CI or headless environments.
pub(crate) fn check_accessibility_permission(
    request_accessibility: bool,
    result: &mut DoctorResult,
) {
    println!(
        "  Current executable: {}",
        std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    );
    print_process("Current process", current_process_info());
    print_process("Parent process", parent_process_info());
    print_process("Grandparent process", grandparent_process_info());

    let trusted = if request_accessibility {
        ax_is_process_trusted_with_prompt()
    } else {
        ax_is_process_trusted()
    };

    if trusted {
        println!("  Accessibility (AXIsProcessTrusted): granted");
        result.pass();
    } else {
        println!("  Accessibility (AXIsProcessTrusted): denied");
        println!("    AXIsProcessTrusted checks the current process, not an arbitrary parent.");
        println!(
            "    Likely process to enable: {}",
            likely_accessibility_process()
        );
        println!("    Open: System Settings -> Privacy & Security -> Accessibility");
        result.fail();
    }
}

fn check_accessibility_smoke(target: Option<&AppTarget>, result: &mut DoctorResult) {
    if ax_api_enabled() {
        println!("  Accessibility API enabled: yes");
        result.pass();
    } else {
        println!("  Accessibility API enabled: no");
        result.fail();
    }

    let system = AXUIElement::system_wide();

    match system.attribute(&accessibility::attribute::AXAttribute::new(
        &CFString::from_static_string("AXFocusedApplication"),
    )) {
        Ok(_app) => {
            println!("  Frontmost application AX: readable");
            result.pass();
        }
        Err(err) => {
            let message = err.to_string();
            if message.contains("APIDisabled") || message.contains("API disabled") {
                println!("  Frontmost application AX: failed with APIDisabled");
                result.fail();
            } else {
                println!("  Frontmost application AX: could not read — {err}");
                result.warn();
            }
        }
    }

    if let Some(target) = target {
        check_target_windows(target, result);
    } else {
        check_finder_windows(result);
    }
}

fn check_target_windows(target: &AppTarget, result: &mut DoctorResult) {
    let probe = MacAppStateProbe::new();
    match probe.pid_for_target(target) {
        Ok(pid) => {
            println!(
                "  {} process: found pid {pid}",
                controller::target_display(target)
            );
            result.pass();
            check_windows_for_pid(&controller::target_display(target), pid, result);
        }
        Err(err) => {
            println!("  {} process: {err}", controller::target_display(target));
            result.fail();
        }
    }
}

fn check_finder_windows(result: &mut DoctorResult) {
    check_windows_for_bundle_id("Finder", "com.apple.finder", result);
}

fn check_windows_for_bundle_id(label: &str, bundle_id: &str, result: &mut DoctorResult) {
    let probe = MacAppStateProbe::new();
    let target = AppTarget::BundleId(bundle_id.to_string());
    match probe.pid_for_target(&target) {
        Ok(pid) => check_windows_for_pid(label, pid, result),
        Err(err) => {
            println!("  {label} AX windows: skipped — {err}");
            result.warn();
        }
    }
}

fn check_windows_for_pid(label: &str, pid: i32, result: &mut DoctorResult) {
    let app = AXUIElement::application(pid);

    match app.windows() {
        Ok(windows) => {
            println!("  {label} AX windows: {} reported", windows.len());
            if let Ok(window) = app.focused_window() {
                let title = window
                    .title()
                    .map(|title| title.to_string())
                    .unwrap_or_else(|_| "untitled".to_string());
                println!("  {label} current window: {title}");
            }
            result.pass();
        }
        Err(err) => {
            let message = err.to_string();
            if message.contains("APIDisabled") || message.contains("API disabled") {
                println!("  {label} AX windows: failed with APIDisabled");
                result.fail();
            } else {
                println!("  {label} AX windows: could not enumerate — {err}");
                result.warn();
            }
        }
    }
}

fn print_process(label: &str, info: Option<controller::ProcessInfo>) {
    match info {
        Some(info) => println!("  {label}: {} (pid {})", info.name, info.pid),
        None => println!("  {label}: unknown"),
    }
}

fn likely_accessibility_process() -> String {
    parent_process_info()
        .map(|info| info.name)
        .unwrap_or_else(|| "the terminal, launcher, or hotkey daemon that invokes summon".into())
}

fn ax_api_enabled() -> bool {
    // SAFETY: `AXAPIEnabled` has no preconditions.
    unsafe { AXAPIEnabled() }
}

fn ax_is_process_trusted() -> bool {
    // SAFETY: `AXIsProcessTrusted` has no preconditions.
    unsafe { AXIsProcessTrusted() }
}

fn ax_is_process_trusted_with_prompt() -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = CFBoolean::true_value();
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    // SAFETY: The dictionary keys and values are valid CoreFoundation objects
    // for the lifetime of the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Expands `~` at the start of a path to the home directory.
fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    } else if let Some(rest) = path.strip_prefix('~') {
        // Collapsed if rejected by pre-commit rustfmt; nested is intentional.
        #[allow(clippy::collapsible_if)]
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}{rest}");
        }
    }
    path.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- DoctorResult ---------------------------------------------------------

    #[test]
    fn doctor_result_new_is_empty() {
        let r = DoctorResult::new();
        assert_eq!(r.checks, 0);
        assert_eq!(r.passed, 0);
        assert_eq!(r.warnings, 0);
        assert_eq!(r.failures, 0);
        assert!(r.is_ok());
    }

    #[test]
    fn doctor_result_pass_increments() {
        let mut r = DoctorResult::new();
        r.pass();
        assert_eq!(r.checks, 1);
        assert_eq!(r.passed, 1);
        assert!(r.is_ok());
    }

    #[test]
    fn doctor_result_warn_increments() {
        let mut r = DoctorResult::new();
        r.warn();
        assert_eq!(r.checks, 1);
        assert_eq!(r.warnings, 1);
        assert!(r.is_ok()); // warnings don't fail
    }

    #[test]
    fn doctor_result_fail_increments() {
        let mut r = DoctorResult::new();
        r.fail();
        assert_eq!(r.checks, 1);
        assert_eq!(r.failures, 1);
        assert!(!r.is_ok());
    }

    #[test]
    fn doctor_result_mixed() {
        let mut r = DoctorResult::new();
        r.pass();
        r.pass();
        r.warn();
        r.fail();
        assert_eq!(r.checks, 4);
        assert_eq!(r.passed, 2);
        assert_eq!(r.warnings, 1);
        assert_eq!(r.failures, 1);
        assert!(!r.is_ok());
    }

    // -- shellexpand_home -----------------------------------------------------

    #[test]
    fn shellexpand_home_without_tilde() {
        assert_eq!(
            shellexpand_home("/Applications/Safari.app"),
            "/Applications/Safari.app"
        );
    }

    // -- format_target_label --------------------------------------------------

    #[test]
    fn format_label_bundle_id() {
        let target = AppTarget::BundleId("com.apple.finder".into());
        assert_eq!(
            format_target_label(&target),
            "bundle identifier \"com.apple.finder\""
        );
    }

    #[test]
    fn format_label_app_name() {
        let target = AppTarget::AppName("Preview".into());
        assert_eq!(format_target_label(&target), "app name \"Preview\"");
    }

    #[test]
    fn format_label_app_path() {
        let target = AppTarget::AppPath("/Applications/Safari.app".into());
        assert_eq!(
            format_target_label(&target),
            "app path \"/Applications/Safari.app\""
        );
    }

    // -- check_config_file with explicit paths --------------------------------
    // These tests avoid env var manipulation by calling check_config_file
    // directly with an explicit PathBuf.

    #[test]
    fn check_config_file_missing() {
        let mut result = DoctorResult::new();
        let path = PathBuf::from("/tmp/summon_test_nonexistent/summon.toml");
        let config = check_config_file(&Some(path), &mut result);

        assert!(config.is_none());
        assert_eq!(result.failures, 1);
        assert_eq!(result.checks, 1);
    }

    #[test]
    fn check_config_file_valid() {
        let dir = std::env::temp_dir().join("summon_test_doctor_check_valid");
        let summon_dir = dir.join("summon");
        std::fs::create_dir_all(&summon_dir).unwrap();

        let path = summon_dir.join("summon.toml");
        std::fs::write(&path, "[bindings.finder]\napp = \"com.apple.finder\"\n").unwrap();

        let mut result = DoctorResult::new();
        let config = check_config_file(&Some(path), &mut result);

        assert!(config.is_some());
        assert_eq!(result.passed, 1);
        assert_eq!(result.failures, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn check_config_file_invalid_toml() {
        let dir = std::env::temp_dir().join("summon_test_doctor_check_invalid");
        let summon_dir = dir.join("summon");
        std::fs::create_dir_all(&summon_dir).unwrap();

        let path = summon_dir.join("summon.toml");
        std::fs::write(&path, "[bindings.broken]\ncycle_when_focused = true\n").unwrap();

        let mut result = DoctorResult::new();
        let config = check_config_file(&Some(path), &mut result);

        assert!(config.is_none());
        assert_eq!(result.failures, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn check_config_file_none_path() {
        let mut result = DoctorResult::new();
        let config = check_config_file(&None, &mut result);

        assert!(config.is_none());
        assert_eq!(result.checks, 0); // skipped, no check counted
    }

    // -- check_bindings with parsed config ------------------------------------

    #[test]
    fn check_bindings_valid_bundle_id() {
        let config = config::parse("[bindings.finder]\napp = \"com.apple.finder\"\n").unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(result.passed, 1);
        assert_eq!(result.failures, 0);
    }

    #[test]
    fn check_bindings_valid_app_name() {
        let config = config::parse("[bindings.preview]\napp = \"Preview\"\n").unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(result.passed, 1);
        assert_eq!(result.failures, 0);
    }

    #[test]
    fn check_bindings_missing_app_path_warns() {
        let config =
            config::parse("[bindings.custom]\napp = \"/Applications/NonExistent.app\"\n").unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(result.warnings, 1, "missing path should warn: {:?}", result);
        assert_eq!(
            result.failures, 0,
            "missing path should not fail: {:?}",
            result
        );
    }

    #[test]
    fn check_bindings_invalid_target_fails() {
        // This config passes config validation (app is non-empty) but the
        // target classifier rejects it because it's a path without .app.
        let toml = "[bindings.bad]\napp = \"/Applications/notanapp\"\n";
        let config = config::parse(toml).unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(
            result.failures, 1,
            "invalid target should fail: {:?}",
            result
        );
    }

    #[test]
    fn check_bindings_empty_config() {
        let config = config::parse("").unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(result.checks, 0, "no bindings = no checks");
    }

    #[test]
    fn check_bindings_multiple_mixed() {
        let config = config::parse(
            r#"
            [bindings.finder]
            app = "com.apple.finder"

            [bindings.preview]
            app = "Preview"

            [bindings.missing]
            app = "/Applications/NonExistent.app"
            "#,
        )
        .unwrap();

        let mut result = DoctorResult::new();
        check_bindings(&config, &mut result);

        assert_eq!(result.passed, 2, "finder + preview should pass");
        assert_eq!(result.warnings, 1, "missing path should warn");
        assert_eq!(result.failures, 0);
    }
}
