//! Configuration loading, parsing, and validation for Summon.
//!
//! Summon reads a TOML config file from `$XDG_CONFIG_HOME/summon/summon.toml`
//! (falling back to `~/.config/summon/summon.toml`). This module provides the
//! config model, path resolution, parsing, and validation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::app::{self, AppTarget};

// ---------------------------------------------------------------------------
// Config model
// ---------------------------------------------------------------------------

/// The supported focus strategies.
///
/// Only `recent-window` is supported in v1. The enum leaves room for future
/// strategies without changing the config schema.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FocusStrategy {
    /// Focus the most recently used window belonging to the target app.
    #[default]
    RecentWindow,
}

/// Global settings that apply to all bindings unless overridden per-binding.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Whether to cycle to the next window when the target app is already focused.
    #[serde(default)]
    pub cycle_when_focused: bool,

    /// Whether to launch the target app if it is not already running.
    #[serde(default)]
    pub launch_if_not_running: bool,

    /// Default focus strategy for all bindings.
    #[serde(default)]
    pub focus_strategy: FocusStrategy,
}

/// A single binding that maps a human-friendly name to an app target.
///
/// Per-binding settings override the corresponding global [`Settings`] values.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// The application target: bundle identifier, app name, or file path.
    pub app: String,

    /// Override global `cycle_when_focused` for this binding.
    #[serde(default)]
    pub cycle_when_focused: Option<bool>,

    /// Override global `launch_if_not_running` for this binding.
    #[serde(default)]
    pub launch_if_not_running: Option<bool>,

    /// Override global `focus_strategy` for this binding.
    #[serde(default)]
    pub focus_strategy: Option<FocusStrategy>,
}

/// The top-level Summon configuration.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Global settings applied to all bindings unless overridden.
    #[serde(default)]
    pub settings: Settings,

    /// Named bindings, each mapping to an application target.
    #[serde(default)]
    pub bindings: BTreeMap<String, Binding>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during config operations.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The `HOME` environment variable is not set.
    #[error("HOME environment variable is not set")]
    NoHome,

    /// The config file could not be read from disk.
    #[error("Could not read config file: {path}\n  {source}")]
    Read {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The config file contains invalid TOML, unknown fields, or wrong types.
    #[error("Invalid config:\n  {0}")]
    Parse(#[from] toml::de::Error),

    /// The config is valid TOML but fails semantic validation.
    #[error("Invalid config:\n  {0}")]
    Validation(String),
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolves the Summon config directory from explicit env values.
///
/// This is a pure function separated from [`config_dir`] for testability.
fn resolve_config_dir(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, ConfigError> {
    match xdg_config_home {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => {
            let h = home.ok_or(ConfigError::NoHome)?;
            Ok(PathBuf::from(h).join(".config"))
        }
    }
}

/// Returns the Summon config directory.
///
/// Uses `$XDG_CONFIG_HOME` if set and non-empty, otherwise `~/.config`.
///
/// # Errors
///
/// Returns [`ConfigError::NoHome`] if neither `XDG_CONFIG_HOME` nor `HOME`
/// is set.
pub fn config_dir() -> Result<PathBuf, ConfigError> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    resolve_config_dir(xdg.as_deref(), home.as_deref())
}

/// Returns the default config file path.
///
/// # Errors
///
/// Returns [`ConfigError::NoHome`] if the config directory cannot be resolved.
pub fn config_path() -> Result<PathBuf, ConfigError> {
    config_dir().map(|dir| dir.join("summon").join("summon.toml"))
}

// ---------------------------------------------------------------------------
// Parsing and loading
// ---------------------------------------------------------------------------

/// Parses a Summon config from a TOML string.
///
/// # Errors
///
/// Returns [`ConfigError::Parse`] for invalid TOML or unknown fields,
/// or [`ConfigError::Validation`] for semantic errors such as an empty `app`.
pub fn parse(toml: &str) -> Result<Config, ConfigError> {
    let config: Config = toml::from_str(toml)?;
    validate(&config)?;
    Ok(config)
}

/// Loads the Summon config from the default path.
///
/// # Errors
///
/// Returns [`ConfigError::NoHome`] if the config directory cannot be resolved,
/// [`ConfigError::Read`] if the file cannot be read, or
/// [`ConfigError::Parse`]/[`ConfigError::Validation`] for invalid content.
pub fn load() -> Result<Config, ConfigError> {
    let path = config_path()?;
    load_from(&path)
}

/// Loads the Summon config from a specific file path.
///
/// # Errors
///
/// Returns [`ConfigError::Read`] if the file cannot be read, or
/// [`ConfigError::Parse`]/[`ConfigError::Validation`] for invalid content.
pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
        path: path.to_path_buf(),
        source: e,
    })?;
    parse(&contents)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validates semantic rules that serde cannot enforce alone.
fn validate(config: &Config) -> Result<(), ConfigError> {
    for (name, binding) in &config.bindings {
        if binding.app.trim().is_empty() {
            return Err(ConfigError::Validation(format!(
                "binding \"{name}\" has an empty app field"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Effective settings resolution
// ---------------------------------------------------------------------------

/// Fully resolved settings for a binding, with per-binding overrides applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectiveSettings {
    /// Whether to cycle to the next window when the target app is already focused.
    pub cycle_when_focused: bool,

    /// Whether to launch the target app if it is not already running.
    pub launch_if_not_running: bool,

    /// Focus strategy to use.
    pub focus_strategy: FocusStrategy,
}

impl EffectiveSettings {
    /// Resolves effective settings by merging global [`Settings`] with per-binding overrides.
    ///
    /// Per-binding values (`Some`) take precedence over global defaults.
    pub fn resolve(global: &Settings, binding: &Binding) -> Self {
        Self {
            cycle_when_focused: binding
                .cycle_when_focused
                .unwrap_or(global.cycle_when_focused),
            launch_if_not_running: binding
                .launch_if_not_running
                .unwrap_or(global.launch_if_not_running),
            focus_strategy: binding.focus_strategy.unwrap_or(global.focus_strategy),
        }
    }
}

// ---------------------------------------------------------------------------
// Binding resolution
// ---------------------------------------------------------------------------

/// A fully resolved binding, ready for action dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedBinding {
    /// The binding name as given on the command line.
    pub name: String,

    /// The classified application target.
    pub target: AppTarget,

    /// The effective settings for this binding.
    pub settings: EffectiveSettings,
}

/// Errors from binding resolution.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// No binding with the requested name exists in the config.
    #[error(
        "Could not resolve binding: {name}\n\
         No binding named \"{name}\" was found in:\n\
         {path}\n\
         Available bindings:\n\
         {available}"
    )]
    BindingNotFound {
        /// The binding name that was requested.
        name: String,
        /// Path to the config file that was searched.
        path: PathBuf,
        /// Formatted list of available binding names.
        available: String,
    },

    /// The app target string could not be classified.
    #[error("Invalid app target for binding \"{binding}\": {source}")]
    InvalidAppTarget {
        /// The binding name.
        binding: String,
        /// The classification error.
        #[source]
        source: app::AppTargetError,
    },
}

/// Looks up a binding by name, classifies its app target, and computes its
/// effective settings.
///
/// # Errors
///
/// Returns [`ResolveError::BindingNotFound`] when the name does not match any
/// key in `config.bindings`, or [`ResolveError::InvalidAppTarget`] when the
/// binding's `app` string cannot be classified.
pub fn resolve_binding(
    config: &Config,
    name: &str,
    config_path: &Path,
) -> Result<ResolvedBinding, ResolveError> {
    let binding = config.bindings.get(name).ok_or_else(|| {
        let available = format_available_bindings(&config.bindings);
        ResolveError::BindingNotFound {
            name: name.to_string(),
            path: config_path.to_path_buf(),
            available,
        }
    })?;

    let target =
        app::classify_app_target(&binding.app).map_err(|e| ResolveError::InvalidAppTarget {
            binding: name.to_string(),
            source: e,
        })?;

    let settings = EffectiveSettings::resolve(&config.settings, binding);

    Ok(ResolvedBinding {
        name: name.to_string(),
        target,
        settings,
    })
}

/// Formats the available binding names for error messages.
fn format_available_bindings(bindings: &BTreeMap<String, Binding>) -> String {
    if bindings.is_empty() {
        "  (none)".to_string()
    } else {
        bindings
            .keys()
            .map(|k| format!("  {k}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;

    // -- Path resolution ----------------------------------------------------

    #[test]
    fn resolve_config_dir_uses_xdg_when_set() {
        let dir = resolve_config_dir(Some("/custom/config"), Some("/Users/test"))
            .expect("should resolve");
        assert_eq!(dir, PathBuf::from("/custom/config"));
    }

    #[test]
    fn resolve_config_dir_ignores_empty_xdg() {
        let dir = resolve_config_dir(Some(""), Some("/Users/test")).expect("should resolve");
        assert_eq!(dir, PathBuf::from("/Users/test/.config"));
    }

    #[test]
    fn resolve_config_dir_falls_back_to_home() {
        let dir = resolve_config_dir(None, Some("/Users/test")).expect("should resolve");
        assert_eq!(dir, PathBuf::from("/Users/test/.config"));
    }

    #[test]
    fn resolve_config_dir_errors_when_no_home() {
        let result = resolve_config_dir(None, None);
        assert!(result.is_err(), "should error without HOME");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ConfigError::NoHome),
            "expected NoHome error, got {err:?}"
        );
    }

    #[test]
    fn config_path_appends_summon_toml() {
        let path = resolve_config_dir(Some("/xdg"), Some("/home"))
            .map(|d| d.join("summon").join("summon.toml"))
            .expect("should resolve");
        assert_eq!(path, PathBuf::from("/xdg/summon/summon.toml"));
    }

    // -- Parsing: minimal / empty -------------------------------------------

    #[test]
    fn parse_empty_toml() {
        let config = parse("").expect("empty TOML should parse");
        assert!(config.bindings.is_empty());
        assert_eq!(config.settings, Settings::default());
    }

    #[test]
    fn parse_bindings_only() {
        let config = parse(
            r#"
            [bindings.terminal]
            app = "com.mitchellh.ghostty"

            [bindings.browser]
            app = "com.brave.Browser"
            "#,
        )
        .expect("should parse");

        assert_eq!(config.bindings.len(), 2);
        assert_eq!(config.bindings["terminal"].app, "com.mitchellh.ghostty");
        assert_eq!(config.bindings["browser"].app, "com.brave.Browser");
    }

    #[test]
    fn parse_full_config() {
        let config = parse(
            r#"
            [settings]
            cycle_when_focused = true
            launch_if_not_running = true
            focus_strategy = "recent-window"

            [bindings.terminal]
            app = "com.mitchellh.ghostty"
            cycle_when_focused = false

            [bindings.browser]
            app = "com.brave.Browser"
            "#,
        )
        .expect("should parse");

        assert!(config.settings.cycle_when_focused);
        assert!(config.settings.launch_if_not_running);
        assert_eq!(config.settings.focus_strategy, FocusStrategy::RecentWindow);

        assert_eq!(config.bindings["terminal"].cycle_when_focused, Some(false));
        assert_eq!(config.bindings["browser"].cycle_when_focused, None);
    }

    // -- Parsing: per-binding overrides --------------------------------------

    #[test]
    fn per_binding_overrides() {
        let config = parse(
            r#"
            [settings]
            cycle_when_focused = true
            launch_if_not_running = false

            [bindings.editor]
            app = "dev.zed.Zed"
            cycle_when_focused = false
            launch_if_not_running = true
            focus_strategy = "recent-window"
            "#,
        )
        .expect("should parse");

        let binding = &config.bindings["editor"];
        assert_eq!(binding.cycle_when_focused, Some(false));
        assert_eq!(binding.launch_if_not_running, Some(true));
        assert_eq!(binding.focus_strategy, Some(FocusStrategy::RecentWindow));
    }

    // -- Parsing: errors ----------------------------------------------------

    #[test]
    fn reject_unknown_settings_field() {
        let result = parse(
            r"
            [settings]
            unknown_field = true
            ",
        );
        assert!(result.is_err(), "should reject unknown settings field");
    }

    #[test]
    fn reject_unknown_binding_field() {
        let result = parse(
            r#"
            [bindings.test]
            app = "com.example.app"
            made_up = "value"
            "#,
        );
        assert!(result.is_err(), "should reject unknown binding field");
    }

    #[test]
    fn reject_unknown_top_level_field() {
        let result = parse(
            r"
            [mystery]
            x = 1
            ",
        );
        assert!(result.is_err(), "should reject unknown top-level field");
    }

    #[test]
    fn reject_invalid_focus_strategy() {
        let result = parse(
            r#"
            [settings]
            focus_strategy = "nonexistent-strategy"
            "#,
        );
        assert!(result.is_err(), "should reject invalid focus strategy");
    }

    #[test]
    fn reject_missing_app_field() {
        let result = parse(
            r"
            [bindings.broken]
            cycle_when_focused = true
            ",
        );
        assert!(result.is_err(), "should reject binding without app");
    }

    // -- Validation ----------------------------------------------------------

    #[test]
    fn reject_empty_app_string() {
        let result = parse(
            r#"
            [bindings.empty]
            app = ""
            "#,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("empty app field"),
            "error should mention empty app: {msg}"
        );
    }

    #[test]
    fn reject_whitespace_only_app() {
        let result = parse(
            r#"
            [bindings.spaces]
            app = "   "
            "#,
        );
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("empty app field"),
            "error should mention empty app: {msg}"
        );
    }

    // -- Load from file ------------------------------------------------------

    #[test]
    fn load_from_file_success() {
        let dir = std::env::temp_dir().join("summon_test_load_from_file");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("summon.toml");
        std::fs::write(
            &path,
            r#"
            [bindings.finder]
            app = "com.apple.finder"
            "#,
        )
        .unwrap();

        let config = load_from(&path).expect("should load from file");
        assert_eq!(config.bindings["finder"].app, "com.apple.finder");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_from_missing_file() {
        let path = std::env::temp_dir().join("summon_nonexistent_summon.toml");
        let result = load_from(&path);
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("Could not read config file"),
            "error should mention file read failure: {err}"
        );
    }

    // -- Config model defaults -----------------------------------------------

    #[test]
    fn settings_defaults() {
        let settings = Settings::default();
        assert!(!settings.cycle_when_focused);
        assert!(!settings.launch_if_not_running);
        assert_eq!(settings.focus_strategy, FocusStrategy::RecentWindow);
    }

    #[test]
    fn binding_option_defaults_are_none() {
        let binding: Binding =
            toml::from_str(r#"app = "com.example.app""#).expect("should parse minimal binding");
        assert_eq!(binding.app, "com.example.app");
        assert_eq!(binding.cycle_when_focused, None);
        assert_eq!(binding.launch_if_not_running, None);
        assert_eq!(binding.focus_strategy, None);
    }

    // -- Config equality -----------------------------------------------------

    #[test]
    fn config_equality() {
        let config = parse(
            r#"
            [bindings.terminal]
            app = "com.mitchellh.ghostty"
            "#,
        )
        .expect("should parse");

        let expected = Config {
            settings: Settings::default(),
            bindings: {
                let mut map = BTreeMap::new();
                map.insert(
                    "terminal".into(),
                    Binding {
                        app: "com.mitchellh.ghostty".into(),
                        cycle_when_focused: None,
                        launch_if_not_running: None,
                        focus_strategy: None,
                    },
                );
                map
            },
        };

        assert_eq!(config, expected);
    }

    // -- Multiple bindings with deterministic order -------------------------

    #[test]
    fn bindings_are_sorted_by_name() {
        let config = parse(
            r#"
            [bindings.zebra]
            app = "com.zebra"

            [bindings.alpha]
            app = "com.alpha"

            [bindings.middle]
            app = "com.middle"
            "#,
        )
        .expect("should parse");

        let names: Vec<&str> = config.bindings.keys().map(String::as_str).collect();
        assert_eq!(names, ["alpha", "middle", "zebra"]);
    }

    // -- Effective settings resolution --------------------------------------

    #[test]
    fn effective_settings_all_defaults() {
        let global = Settings::default();
        let binding = Binding {
            app: "com.example.app".into(),
            cycle_when_focused: None,
            launch_if_not_running: None,
            focus_strategy: None,
        };

        let effective = EffectiveSettings::resolve(&global, &binding);
        assert!(!effective.cycle_when_focused);
        assert!(!effective.launch_if_not_running);
        assert_eq!(effective.focus_strategy, FocusStrategy::RecentWindow);
    }

    #[test]
    fn effective_settings_global_values_used() {
        let global = Settings {
            cycle_when_focused: true,
            launch_if_not_running: true,
            focus_strategy: FocusStrategy::RecentWindow,
        };
        let binding = Binding {
            app: "com.example.app".into(),
            cycle_when_focused: None,
            launch_if_not_running: None,
            focus_strategy: None,
        };

        let effective = EffectiveSettings::resolve(&global, &binding);
        assert!(effective.cycle_when_focused);
        assert!(effective.launch_if_not_running);
    }

    #[test]
    fn effective_settings_per_binding_overrides_global() {
        let global = Settings {
            cycle_when_focused: true,
            launch_if_not_running: true,
            focus_strategy: FocusStrategy::RecentWindow,
        };
        let binding = Binding {
            app: "com.example.app".into(),
            cycle_when_focused: Some(false),
            launch_if_not_running: Some(false),
            focus_strategy: None,
        };

        let effective = EffectiveSettings::resolve(&global, &binding);
        assert!(!effective.cycle_when_focused);
        assert!(!effective.launch_if_not_running);
        assert_eq!(effective.focus_strategy, FocusStrategy::RecentWindow);
    }

    #[test]
    fn effective_settings_partial_overrides() {
        let global = Settings {
            cycle_when_focused: true,
            launch_if_not_running: false,
            focus_strategy: FocusStrategy::RecentWindow,
        };
        let binding = Binding {
            app: "com.example.app".into(),
            cycle_when_focused: None,
            launch_if_not_running: Some(true),
            focus_strategy: None,
        };

        let effective = EffectiveSettings::resolve(&global, &binding);
        assert!(effective.cycle_when_focused); // from global
        assert!(effective.launch_if_not_running); // from per-binding override
    }

    // -- Binding resolution -------------------------------------------------

    #[test]
    fn resolve_binding_found() {
        let config = parse(
            r#"
            [settings]
            cycle_when_focused = true

            [bindings.terminal]
            app = "com.mitchellh.ghostty"
            "#,
        )
        .expect("should parse");

        let path = PathBuf::from("/test/summon.toml");
        let resolved = resolve_binding(&config, "terminal", &path).expect("should resolve");

        assert_eq!(resolved.name, "terminal");
        assert_eq!(
            resolved.target,
            AppTarget::BundleId("com.mitchellh.ghostty".into())
        );
        assert!(resolved.settings.cycle_when_focused);
    }

    #[test]
    fn resolve_binding_not_found() {
        let config = parse(
            r#"
            [bindings.browser]
            app = "com.brave.Browser"

            [bindings.editor]
            app = "dev.zed.Zed"
            "#,
        )
        .expect("should parse");

        let path = PathBuf::from("/test/summon.toml");
        let result = resolve_binding(&config, "terminal", &path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Could not resolve binding: terminal"),
            "error should mention binding name: {msg}"
        );
        assert!(
            msg.contains("/test/summon.toml"),
            "error should mention config path: {msg}"
        );
        assert!(
            msg.contains("browser") && msg.contains("editor"),
            "error should list available bindings: {msg}"
        );
    }

    #[test]
    fn resolve_binding_not_found_empty_config() {
        let config = parse("").expect("should parse empty config");
        let path = PathBuf::from("/test/summon.toml");
        let result = resolve_binding(&config, "anything", &path);

        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("(none)"),
            "error should indicate no bindings available: {msg}"
        );
    }

    #[test]
    fn resolve_binding_inherits_per_binding_override() {
        let config = parse(
            r#"
            [settings]
            cycle_when_focused = true
            launch_if_not_running = false

            [bindings.editor]
            app = "dev.zed.Zed"
            launch_if_not_running = true
            "#,
        )
        .expect("should parse");

        let path = PathBuf::from("/test/summon.toml");
        let resolved = resolve_binding(&config, "editor", &path).expect("should resolve");

        assert!(resolved.settings.cycle_when_focused); // inherited from global
        assert!(resolved.settings.launch_if_not_running); // overridden per-binding
    }

    // -- App target classification in resolve -------------------------------

    #[test]
    fn resolve_binding_classifies_bundle_id() {
        let config = parse(
            r#"
            [bindings.finder]
            app = "com.apple.finder"
            "#,
        )
        .expect("should parse");

        let resolved =
            resolve_binding(&config, "finder", Path::new("/test.toml")).expect("should resolve");
        assert_eq!(
            resolved.target,
            AppTarget::BundleId("com.apple.finder".into())
        );
    }

    #[test]
    fn resolve_binding_classifies_app_name() {
        let config = parse(
            r#"
            [bindings.preview]
            app = "Preview"
            "#,
        )
        .expect("should parse");

        let resolved =
            resolve_binding(&config, "preview", Path::new("/test.toml")).expect("should resolve");
        assert_eq!(resolved.target, AppTarget::AppName("Preview".into()));
    }

    #[test]
    fn resolve_binding_classifies_app_path() {
        let config = parse(
            r#"
            [bindings.custom]
            app = "/Applications/My App.app"
            "#,
        )
        .expect("should parse");

        let resolved =
            resolve_binding(&config, "custom", Path::new("/test.toml")).expect("should resolve");
        assert_eq!(
            resolved.target,
            AppTarget::AppPath("/Applications/My App.app".into())
        );
    }

    #[test]
    fn resolve_binding_rejects_invalid_path() {
        let config = parse(
            r#"
            [bindings.bad]
            app = "/Applications/notanapp"
            "#,
        )
        .expect("should parse");

        let result = resolve_binding(&config, "bad", Path::new("/test.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("bad"),
            "error should mention binding name: {msg}"
        );
        assert!(
            msg.contains(".app"),
            "error should mention .app extension: {msg}"
        );
    }

    // -- Available bindings formatting --------------------------------------

    #[test]
    fn format_available_multiple_bindings() {
        let config = parse(
            r#"
            [bindings.browser]
            app = "com.brave.Browser"

            [bindings.terminal]
            app = "com.mitchellh.ghostty"
            "#,
        )
        .expect("should parse");

        let output = format_available_bindings(&config.bindings);
        assert!(output.contains("browser"));
        assert!(output.contains("terminal"));
        // BTreeMap gives sorted order
        let browser_pos = output.find("browser").expect("should find browser");
        let terminal_pos = output.find("terminal").expect("should find terminal");
        assert!(
            browser_pos < terminal_pos,
            "browser should appear before terminal"
        );
    }

    #[test]
    fn format_available_empty_bindings() {
        let bindings = BTreeMap::new();
        let output = format_available_bindings(&bindings);
        assert_eq!(output, "  (none)");
    }
}
