//! Application target resolution for Summon.
//!
//! Classifies the `app` field string from config bindings into a typed
//! [`AppTarget`]: bundle identifier, application name, or application path.

use std::path::Path;

use thiserror::Error;

// ---------------------------------------------------------------------------
// AppTarget
// ---------------------------------------------------------------------------

/// A classified application target.
///
/// Summon resolves apps by bundle identifier (preferred), exact application
/// name, or application path. This enum represents the result of classifying
/// the `app` string from a binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppTarget {
    /// A macOS bundle identifier (e.g. `com.apple.finder`, `dev.zed.Zed`).
    BundleId(String),

    /// An application name (e.g. `Preview`, `Safari`).
    AppName(String),

    /// An application path (e.g. `/Applications/My App.app`).
    AppPath(String),
}

/// Errors from app target classification.
#[derive(Debug, Error)]
pub enum AppTargetError {
    /// The app string looks like a path but does not end in `.app`.
    #[error("Application path does not have .app extension: {path}")]
    InvalidAppPath {
        /// The path that was provided.
        path: String,
    },
}

/// Classifies an `app` string from config into an [`AppTarget`].
///
/// Classification rules (evaluated in order):
///
/// 1. **Path**: if the string starts with `/` or `~`, it is treated as a path.
///    Paths must end with `.app`.
/// 2. **Bundle identifier**: if the string contains at least one `.` and each
///    segment between dots is non-empty, it is treated as a bundle identifier.
/// 3. **Application name**: otherwise, the string is treated as an application
///    name.
///
/// # Errors
///
/// Returns [`AppTargetError::InvalidAppPath`] when the string looks like a path
/// (starts with `/` or `~`) but does not end with `.app`.
pub fn classify_app_target(app: &str) -> Result<AppTarget, AppTargetError> {
    let trimmed = app.trim();

    if trimmed.starts_with('/') || trimmed.starts_with('~') {
        if looks_like_app_path(trimmed) {
            return Ok(AppTarget::AppPath(trimmed.to_string()));
        }
        return Err(AppTargetError::InvalidAppPath {
            path: trimmed.to_string(),
        });
    }

    if looks_like_bundle_id(trimmed) {
        return Ok(AppTarget::BundleId(trimmed.to_string()));
    }

    Ok(AppTarget::AppName(trimmed.to_string()))
}

/// Returns `true` if the string has the structural shape of a bundle identifier.
///
/// A bundle identifier contains at least one dot, and every segment between
/// dots is non-empty and contains only alphanumeric characters, hyphens, and
/// underscores. At least one segment must contain a letter (to distinguish from
/// e.g. version strings like `1.2.3`).
fn looks_like_bundle_id(s: &str) -> bool {
    if !s.contains('.') {
        return false;
    }

    let segments: Vec<&str> = s.split('.').collect();
    if segments.len() < 2 {
        return false;
    }

    let mut has_letter = false;
    for segment in &segments {
        if segment.is_empty() {
            return false;
        }
        for ch in segment.chars() {
            if ch.is_ascii_alphabetic() {
                has_letter = true;
            } else if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return false;
            }
        }
    }

    has_letter
}

/// Returns `true` if the string looks like an application path (ends with `.app`).
fn looks_like_app_path(s: &str) -> bool {
    let expanded = strip_tilde(s);
    Path::new(&expanded)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
}

/// Strips a leading `~/` from a path string for extension checking.
fn strip_tilde(s: &str) -> &str {
    s.strip_prefix("~/").unwrap_or(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    // -- Bundle identifiers --------------------------------------------------

    #[test]
    fn classify_standard_bundle_id() {
        let target = classify_app_target("com.apple.finder").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("com.apple.finder".into()));
    }

    #[test]
    fn classify_bundle_id_with_hyphens() {
        let target = classify_app_target("com.tinyspeck.slackmacgap").expect("should classify");
        assert_eq!(
            target,
            AppTarget::BundleId("com.tinyspeck.slackmacgap".into())
        );
    }

    #[test]
    fn classify_bundle_id_with_underscores() {
        let target = classify_app_target("org.my_app.component").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("org.my_app.component".into()));
    }

    #[test]
    fn classify_short_bundle_id() {
        let target = classify_app_target("dev.zed.Zed").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("dev.zed.Zed".into()));
    }

    #[test]
    fn classify_two_segment_bundle_id() {
        let target = classify_app_target("com.brave.Browser").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("com.brave.Browser".into()));
    }

    #[test]
    fn classify_bundle_id_is_case_insensitive() {
        let target = classify_app_target("Com.Example.App").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("Com.Example.App".into()));
    }

    // -- Application names ---------------------------------------------------

    #[test]
    fn classify_single_word_app_name() {
        let target = classify_app_target("Preview").expect("should classify");
        assert_eq!(target, AppTarget::AppName("Preview".into()));
    }

    #[test]
    fn classify_multi_word_app_name() {
        let target = classify_app_target("Visual Studio Code").expect("should classify");
        assert_eq!(target, AppTarget::AppName("Visual Studio Code".into()));
    }

    #[test]
    fn classify_app_name_with_special_chars() {
        let target = classify_app_target("My App (2024)").expect("should classify");
        assert_eq!(target, AppTarget::AppName("My App (2024)".into()));
    }

    // -- Application paths ---------------------------------------------------

    #[test]
    fn classify_absolute_app_path() {
        let target =
            classify_app_target("/Applications/My Custom App.app").expect("should classify");
        assert_eq!(
            target,
            AppTarget::AppPath("/Applications/My Custom App.app".into())
        );
    }

    #[test]
    fn classify_tilde_app_path() {
        let target = classify_app_target("~/Applications/My App.app").expect("should classify");
        assert_eq!(
            target,
            AppTarget::AppPath("~/Applications/My App.app".into())
        );
    }

    #[test]
    fn classify_standard_applications_path() {
        let target = classify_app_target("/Applications/Safari.app").expect("should classify");
        assert_eq!(
            target,
            AppTarget::AppPath("/Applications/Safari.app".into())
        );
    }

    // -- Path without .app extension ----------------------------------------

    #[test]
    fn reject_path_without_app_extension() {
        let result = classify_app_target("/Applications/Safari");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains(".app"),
            "error should mention .app extension: {msg}"
        );
    }

    #[test]
    fn reject_tilde_path_without_app_extension() {
        let result = classify_app_target("~/Apps/Launcher");
        assert!(result.is_err());
    }

    // -- Edge cases ----------------------------------------------------------

    #[test]
    fn version_string_treated_as_app_name() {
        // "1.2.3" has dots but no letters in segments, so it's not a bundle ID.
        let target = classify_app_target("1.2.3").expect("should classify");
        assert_eq!(target, AppTarget::AppName("1.2.3".into()));
    }

    #[test]
    fn single_name_treated_as_app_name() {
        let target = classify_app_target("Finder").expect("should classify");
        assert_eq!(target, AppTarget::AppName("Finder".into()));
    }

    #[test]
    fn empty_after_dot_treated_as_app_name() {
        // "app." has an empty trailing segment, so not a bundle ID
        let target = classify_app_target("app.").expect("should classify");
        assert_eq!(target, AppTarget::AppName("app.".into()));
    }

    #[test]
    fn dot_prefixed_treated_as_app_name() {
        // ".app" has an empty leading segment
        let target = classify_app_target(".app").expect("should classify");
        assert_eq!(target, AppTarget::AppName(".app".into()));
    }

    #[test]
    fn spaces_in_name_treated_as_app_name() {
        let target = classify_app_target("My App").expect("should classify");
        assert_eq!(target, AppTarget::AppName("My App".into()));
    }

    // -- Bundle ID heuristic edge cases --------------------------------------

    #[test]
    fn bundle_id_with_uppercase_segments() {
        let target = classify_app_target("Dev.Zed.Zed").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("Dev.Zed.Zed".into()));
    }

    #[test]
    fn bundle_id_all_digits_rejected() {
        // "123.456" has no letters in any segment → treated as app name
        let target = classify_app_target("123.456").expect("should classify");
        assert_eq!(target, AppTarget::AppName("123.456".into()));
    }

    #[test]
    fn bundle_id_mixed_digits_and_letters() {
        let target = classify_app_target("com.example.app2").expect("should classify");
        assert_eq!(target, AppTarget::BundleId("com.example.app2".into()));
    }

    // -- Helper tests --------------------------------------------------------

    #[test]
    fn looks_like_bundle_id_standard() {
        assert!(looks_like_bundle_id("com.apple.finder"));
    }

    #[test]
    fn looks_like_bundle_id_no_dot() {
        assert!(!looks_like_bundle_id("Finder"));
    }

    #[test]
    fn looks_like_bundle_id_empty_segment() {
        assert!(!looks_like_bundle_id("com..finder"));
    }

    #[test]
    fn looks_like_bundle_id_special_chars() {
        assert!(!looks_like_bundle_id("com.apple/find"));
    }

    #[test]
    fn strip_tilde_from_home_path() {
        assert_eq!(
            strip_tilde("~/Applications/App.app"),
            "Applications/App.app"
        );
    }

    #[test]
    fn strip_tilde_no_tilde() {
        assert_eq!(
            strip_tilde("/Applications/App.app"),
            "/Applications/App.app"
        );
    }
}
