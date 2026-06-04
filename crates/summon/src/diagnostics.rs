//! Diagnostics for Summon.
//!
//! The [`run_doctor`] function checks the health of the Summon installation,
//! including config readability, binding validity, app target resolution, and
//! macOS Accessibility permissions.

use std::path::{Path, PathBuf};

use crate::app::{self, AppTarget};
use crate::config;

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
pub fn run_doctor() -> DoctorResult {
    let mut result = DoctorResult::new();

    let config_path = check_config_path(&mut result);

    let config = check_config_file(&config_path, &mut result);

    if let Some(config) = config.as_ref() {
        check_bindings(config, &mut result);
    }

    check_accessibility_permission(&mut result);

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

/// Checks macOS Accessibility permission by attempting an AppleScript query.
///
/// Reports a failure only when the permission is definitively denied.
/// Timeouts and other errors are reported as warnings so they don't fail
/// the overall `summon doctor` run in CI or headless environments.
pub(crate) fn check_accessibility_permission(result: &mut DoctorResult) {
    let script = "tell application \"System Events\" to get name of first process";
    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            println!("  Accessibility: granted");
            result.pass();
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("not allowed") || stderr.contains("not authorized") {
                println!("  Accessibility: denied");
                println!("    Summon needs Accessibility permission to focus application windows.");
                println!("    Open:");
                println!("      System Settings → Privacy & Security → Accessibility");
                println!(
                    "    Then enable the terminal, launcher, or hotkey daemon that invokes Summon."
                );
                result.fail();
            } else {
                // Timeouts and other errors are not definitive permission failures.
                println!("  Accessibility: could not verify — {}", stderr.trim());
                result.warn();
            }
        }
        Err(err) => {
            println!("  Accessibility: could not check — {err}");
            result.warn();
        }
    }
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
