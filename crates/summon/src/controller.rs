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
//! 4. If the app is frontmost and cycling is disabled, return [`AppAction::AlreadyFocused`].
//! 5. If the app is not running and `launch_if_not_running` is false, return
//!    [`AppAction::LaunchDisabled`] (nothing to do).

#![allow(unexpected_cfgs)]

use crate::app::AppTarget;
use crate::config::EffectiveSettings;

use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use accessibility::action::AXUIElementActions;
use accessibility::attribute::AXUIElementAttributes;
use accessibility::ui_element::AXUIElement;
use accessibility::Error as AXError;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use thiserror::Error;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

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
    /// Do nothing because the app is already frontmost and cycling is disabled.
    AlreadyFocused,
    /// Do nothing because the app is not running and launching is disabled.
    LaunchDisabled,
}

/// The observed frontmost state used while deciding an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedFrontmost {
    /// The app is frontmost.
    Yes,
    /// The app is running but not frontmost.
    No,
    /// Frontmost state was not checked because the app was not running.
    NotChecked,
}

/// Facts used to choose an [`AppAction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionContext {
    /// Whether the target app is running.
    pub is_running: bool,
    /// Whether the target app is currently frontmost.
    pub frontmost: ObservedFrontmost,
    /// Effective setting: launch app if not running.
    pub launch_when_missing: bool,
    /// Effective setting: cycle windows when app is already focused.
    pub cycle_when_focused: bool,
}

/// Errors from querying or controlling a macOS application.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ControllerError {
    /// The target app could not be launched.
    #[error("Could not launch {target}: {reason}")]
    LaunchFailed {
        /// Human-readable app target.
        target: String,
        /// Underlying failure.
        reason: String,
    },

    /// The target app could not be focused.
    #[error("Could not focus {target}: {reason}")]
    FocusFailed {
        /// Human-readable app target.
        target: String,
        /// Underlying failure.
        reason: String,
    },

    /// The current process is not trusted for Accessibility.
    #[error(
        "Accessibility permission denied for the current process.\n  Current executable: {executable}\n  Parent process: {parent}\n  Likely process to enable: {likely_process}\n  Open: System Settings -> Privacy & Security -> Accessibility"
    )]
    PermissionDenied {
        /// Current executable path.
        executable: String,
        /// Parent process details.
        parent: String,
        /// Best-effort process to enable in System Settings.
        likely_process: String,
    },

    /// The target process could not be found.
    #[error("Could not find PID for {target}")]
    PidLookupFailed {
        /// Human-readable app target.
        target: String,
    },

    /// The target app has no windows.
    #[error("{target} has no windows to cycle")]
    NoWindows {
        /// Human-readable app target.
        target: String,
    },

    /// The target app has only one window after filtering.
    #[error("{target} has only one cyclable window, so there is nothing to cycle")]
    OnlyOneCyclableWindow {
        /// Human-readable app target.
        target: String,
        /// Window title, if available.
        title: Option<String>,
    },

    /// The target app has windows, but none are suitable for default cycling.
    #[error(
        "No cyclable windows for {target} ({total_windows} total, {rejected_windows} rejected)"
    )]
    NoCyclableWindows {
        /// Human-readable app target.
        target: String,
        /// Total AX windows reported by the app.
        total_windows: usize,
        /// Number filtered out.
        rejected_windows: usize,
    },

    /// macOS accepted a raise request but did not report the target as current.
    #[error("Raised a window for {target}, but macOS did not report it as focused afterwards")]
    RaiseVerificationFailed {
        /// Human-readable app target.
        target: String,
        /// Window title, if available.
        title: Option<String>,
    },

    /// The Accessibility API returned an unexpected error.
    #[error("Accessibility API error: {0}")]
    AxApi(String),
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
    fn launch(&self, target: &AppTarget) -> Result<(), ControllerError>;

    /// Brings the target application to the foreground.
    ///
    /// # Errors
    ///
    /// Implementations should return a descriptive error if the app cannot be
    /// focused (not running, Accessibility permission missing, etc.).
    fn focus(&self, target: &AppTarget) -> Result<(), ControllerError>;

    /// Cycles to the next window belonging to the target application.
    ///
    /// # Errors
    ///
    /// Implementations should return a descriptive error if cycling fails.
    fn cycle_window(&self, target: &AppTarget) -> Result<(), ControllerError>;
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
/// | Running | Frontmost | Launch? | Cycle? | Action         |
/// |---------|-----------|---------|--------|----------------|
/// | No      | -         | Yes     | -      | Launch         |
/// | No      | -         | No      | -      | LaunchDisabled |
/// | Yes     | No        | -       | -      | Focus          |
/// | Yes     | Yes       | -       | Yes    | Cycle          |
/// | Yes     | Yes       | -       | No     | AlreadyFocused |
pub fn decide_action(
    controller: &dyn AppController,
    target: &AppTarget,
    settings: &EffectiveSettings,
) -> (AppAction, DecisionContext) {
    let is_running = controller.is_running(target);
    let frontmost = if is_running {
        if controller.is_frontmost(target) {
            ObservedFrontmost::Yes
        } else {
            ObservedFrontmost::No
        }
    } else {
        ObservedFrontmost::NotChecked
    };

    let context = DecisionContext {
        is_running,
        frontmost,
        launch_when_missing: settings.launch_if_not_running,
        cycle_when_focused: settings.cycle_when_focused,
    };

    let action = if !is_running {
        if settings.launch_if_not_running {
            AppAction::Launch
        } else {
            AppAction::LaunchDisabled
        }
    } else if frontmost == ObservedFrontmost::No {
        AppAction::Focus
    } else if settings.cycle_when_focused {
        AppAction::Cycle
    } else {
        AppAction::AlreadyFocused
    };

    (action, context)
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
) -> Result<(), ControllerError> {
    match action {
        AppAction::Launch => controller.launch(target),
        AppAction::Focus => controller.focus(target),
        AppAction::Cycle => controller.cycle_window(target),
        AppAction::AlreadyFocused | AppAction::LaunchDisabled => Ok(()),
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
/// let (action, _context) = decide_action(&controller, &target, &settings);
/// assert_eq!(action, AppAction::LaunchDisabled); // launch_if_not_running defaults to false
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

    fn launch(&self, _target: &AppTarget) -> Result<(), ControllerError> {
        Ok(())
    }

    fn focus(&self, _target: &AppTarget) -> Result<(), ControllerError> {
        Ok(())
    }

    fn cycle_window(&self, _target: &AppTarget) -> Result<(), ControllerError> {
        Ok(())
    }
}

/// Read-only app state queries.
pub trait AppStateProbe {
    /// Returns `true` if the target application is currently running.
    fn is_running(&self, target: &AppTarget) -> Result<bool, ControllerError>;

    /// Returns `true` if the target application is currently frontmost.
    fn is_frontmost(&self, target: &AppTarget) -> Result<bool, ControllerError>;

    /// Returns the process identifier for the target application.
    fn pid_for_target(&self, target: &AppTarget) -> Result<i32, ControllerError>;
}

/// Native window cycling backend.
pub trait WindowCycler {
    /// Cycles to the next window belonging to the target application.
    fn cycle_window(&self, target: &AppTarget, pid: i32) -> Result<(), ControllerError>;
}

// ---------------------------------------------------------------------------
// MacAppController
// ---------------------------------------------------------------------------

/// A macOS app controller that uses `open`, `NSWorkspace`, and native AX.
///
/// This controller uses the macOS `open` command for launching and focusing
/// apps, `NSWorkspace` for lightweight state queries, and native Accessibility
/// APIs for window mutation.
pub struct MacAppController {
    state_probe: MacAppStateProbe,
    window_cycler: MacWindowCycler,
}

/// `NSWorkspace`-backed read-only macOS app state probe.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacAppStateProbe;

/// AXUIElement-backed macOS window cycler.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacWindowCycler;

impl MacAppController {
    /// Creates a new macOS app controller.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state_probe: MacAppStateProbe,
            window_cycler: MacWindowCycler,
        }
    }
}

impl MacAppStateProbe {
    /// Creates a new macOS app state probe.
    #[must_use]
    pub fn new() -> Self {
        Self
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

    fn running_apps_for_target(&self, target: &AppTarget) -> Vec<RunningApp> {
        running_apps()
            .into_iter()
            .filter(|app| app_matches_target(app, target))
            .collect()
    }

    fn frontmost_app(&self) -> Option<RunningApp> {
        frontmost_running_app()
    }
}

impl Default for MacAppController {
    fn default() -> Self {
        Self::new()
    }
}

impl AppStateProbe for MacAppStateProbe {
    fn is_running(&self, target: &AppTarget) -> Result<bool, ControllerError> {
        Ok(!self.running_apps_for_target(target).is_empty())
    }

    fn is_frontmost(&self, target: &AppTarget) -> Result<bool, ControllerError> {
        Ok(self
            .frontmost_app()
            .is_some_and(|app| app_matches_target(&app, target)))
    }

    fn pid_for_target(&self, target: &AppTarget) -> Result<i32, ControllerError> {
        let apps = self.running_apps_for_target(target);
        let frontmost_pid = self
            .frontmost_app()
            .filter(|app| app_matches_target(app, target))
            .map(|app| app.pid);

        frontmost_pid
            .or_else(|| apps.first().map(|app| app.pid))
            .ok_or_else(|| ControllerError::PidLookupFailed {
                target: target_display(target),
            })
    }
}

impl AppController for MacAppController {
    fn is_running(&self, target: &AppTarget) -> bool {
        self.state_probe.is_running(target).unwrap_or(false)
    }

    fn is_frontmost(&self, target: &AppTarget) -> bool {
        self.state_probe.is_frontmost(target).unwrap_or(false)
    }

    fn launch(&self, target: &AppTarget) -> Result<(), ControllerError> {
        if let AppTarget::BundleId(id) = target {
            return launch_bundle_id(id).map_err(|reason| ControllerError::LaunchFailed {
                target: target_display(target),
                reason,
            });
        }

        let result = match target {
            AppTarget::BundleId(_) => unreachable!("bundle ids are handled above"),
            AppTarget::AppName(name) => std::process::Command::new("open")
                .args(["-a", name])
                .output()
                .map_err(|e| ControllerError::LaunchFailed {
                    target: target_display(target),
                    reason: format!("Failed to run open: {e}"),
                })?,
            AppTarget::AppPath(path) => std::process::Command::new("open")
                .arg(path)
                .output()
                .map_err(|e| ControllerError::LaunchFailed {
                    target: target_display(target),
                    reason: format!("Failed to run open: {e}"),
                })?,
        };

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(ControllerError::LaunchFailed {
                target: target_display(target),
                reason: format_app_error(target, stderr.trim()),
            });
        }

        Ok(())
    }

    fn focus(&self, target: &AppTarget) -> Result<(), ControllerError> {
        // `open` brings the app to the foreground whether it was just launched
        // or was already running, so launch and focus share the same mechanism.
        self.launch(target).map_err(|err| match err {
            ControllerError::LaunchFailed { target, reason } => {
                ControllerError::FocusFailed { target, reason }
            }
            other => other,
        })
    }

    fn cycle_window(&self, target: &AppTarget) -> Result<(), ControllerError> {
        let pid = self.state_probe.pid_for_target(target)?;
        self.window_cycler.cycle_window(target, pid)
    }
}

fn launch_bundle_id(bundle_id: &str) -> Result<(), String> {
    let output = std::process::Command::new("open")
        .args(["-b", bundle_id])
        .output()
        .map_err(|e| format!("Failed to run open: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let open_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let target = AppTarget::BundleId(bundle_id.to_string());
    match MacAppStateProbe::new().pid_for_target(&target) {
        Ok(pid) if activate_pid(pid) => return Ok(()),
        _ => {}
    }

    let fallback_path = find_app_by_bundle_id(bundle_id);
    let Some(path) = fallback_path else {
        return Err(format!(
            "Could not open app with bundle identifier \"{bundle_id}\": {open_error}"
        ));
    };

    register_app_with_launch_services(&path);
    let retry_output = std::process::Command::new("open")
        .args(["-b", bundle_id])
        .output()
        .map_err(|e| {
            format!(
                "Failed to retry open after registering {}: {e}",
                path.display()
            )
        })?;
    if retry_output.status.success() {
        return Ok(());
    }
    let retry_error = String::from_utf8_lossy(&retry_output.stderr)
        .trim()
        .to_string();

    let fallback_output = std::process::Command::new("open")
        .arg(&path)
        .output()
        .map_err(|e| format!("Failed to run open fallback for {}: {e}", path.display()))?;

    if fallback_output.status.success() {
        return Ok(());
    }

    let fallback_error = String::from_utf8_lossy(&fallback_output.stderr)
        .trim()
        .to_string();
    if launch_app_executable(&path).is_ok() {
        return Ok(());
    }

    Err(format!(
        "Could not open app with bundle identifier \"{bundle_id}\": {open_error}; retry after registering {} failed: {retry_error}; fallback {} failed: {fallback_error}",
        path.display(),
        path.display()
    ))
}

fn activate_pid(pid: i32) -> bool {
    // SAFETY: This uses AppKit's NSRunningApplication activation API for a PID
    // already returned by NSWorkspace. A null object means the app disappeared.
    unsafe {
        let app: *mut Object =
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if app.is_null() {
            return false;
        }

        const NS_APPLICATION_ACTIVATE_ALL_WINDOWS: usize = 1 << 0;
        const NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS: usize = 1 << 1;
        let options =
            NS_APPLICATION_ACTIVATE_ALL_WINDOWS | NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS;
        let activated: bool = msg_send![app, activateWithOptions: options];
        activated
    }
}

fn find_app_by_bundle_id(bundle_id: &str) -> Option<PathBuf> {
    app_search_roots()
        .into_iter()
        .flat_map(|root| app_dirs_under(&root, 3))
        .find(|app| app_bundle_id(app).as_deref() == Some(bundle_id))
}

fn register_app_with_launch_services(app_path: &Path) {
    let _ = std::process::Command::new(
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
    )
    .args(["-f"])
    .arg(app_path)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status();
}

fn app_search_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}

fn app_dirs_under(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut apps = Vec::new();
    collect_app_dirs(root, max_depth, &mut apps);
    apps
}

fn collect_app_dirs(path: &Path, depth_remaining: usize, apps: &mut Vec<PathBuf>) {
    if depth_remaining == 0 || !path.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        {
            apps.push(path);
        } else if path.is_dir() {
            collect_app_dirs(&path, depth_remaining - 1, apps);
        }
    }
}

fn app_bundle_id(app_path: &Path) -> Option<String> {
    app_info_plist_value(app_path, "CFBundleIdentifier")
}

fn app_executable_path(app_path: &Path) -> Option<PathBuf> {
    let executable_name = app_info_plist_value(app_path, "CFBundleExecutable")?;
    let executable = app_path.join("Contents/MacOS").join(executable_name);
    executable.is_file().then_some(executable)
}

fn app_info_plist_value(app_path: &Path, key: &str) -> Option<String> {
    let info_plist = app_path.join("Contents/Info.plist");
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(info_plist)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn launch_app_executable(app_path: &Path) -> Result<(), String> {
    let executable = app_executable_path(app_path)
        .ok_or_else(|| format!("Could not find executable for {}", app_path.display()))?;

    std::process::Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to run {}: {e}", executable.display()))
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

/// Formats a target for user-facing output.
pub fn target_display(target: &AppTarget) -> String {
    match target {
        AppTarget::BundleId(id) => id.clone(),
        AppTarget::AppName(name) => name.clone(),
        AppTarget::AppPath(path) => path.clone(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RunningApp {
    pid: i32,
    bundle_id: Option<String>,
    localized_name: Option<String>,
    bundle_path: Option<String>,
}

fn running_apps() -> Vec<RunningApp> {
    // SAFETY: Objective-C messages are sent to documented AppKit/Foundation
    // classes. Returned objects are only inspected during the call.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return Vec::new();
        }

        let apps: *mut Object = msg_send![workspace, runningApplications];
        nsarray_to_running_apps(apps)
    }
}

fn frontmost_running_app() -> Option<RunningApp> {
    // SAFETY: Objective-C messages are sent to documented AppKit/Foundation
    // classes. Returned objects are only inspected during the call.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }

        let app: *mut Object = msg_send![workspace, frontmostApplication];
        running_app_from_ns(app)
    }
}

unsafe fn nsarray_to_running_apps(apps: *mut Object) -> Vec<RunningApp> {
    if apps.is_null() {
        return Vec::new();
    }

    let count: usize = unsafe { msg_send![apps, count] };
    let mut result = Vec::with_capacity(count);
    for idx in 0..count {
        let app: *mut Object = unsafe { msg_send![apps, objectAtIndex: idx] };
        if let Some(app) = unsafe { running_app_from_ns(app) } {
            result.push(app);
        }
    }
    result
}

unsafe fn running_app_from_ns(app: *mut Object) -> Option<RunningApp> {
    if app.is_null() {
        return None;
    }

    let pid: i32 = unsafe { msg_send![app, processIdentifier] };
    let bundle_id: *mut Object = unsafe { msg_send![app, bundleIdentifier] };
    let localized_name: *mut Object = unsafe { msg_send![app, localizedName] };
    let bundle_url: *mut Object = unsafe { msg_send![app, bundleURL] };
    let bundle_path = if bundle_url.is_null() {
        None
    } else {
        let path: *mut Object = unsafe { msg_send![bundle_url, path] };
        unsafe { nsstring_to_string(path) }
    };

    Some(RunningApp {
        pid,
        bundle_id: unsafe { nsstring_to_string(bundle_id) },
        localized_name: unsafe { nsstring_to_string(localized_name) },
        bundle_path,
    })
}

unsafe fn nsstring_to_string(value: *mut Object) -> Option<String> {
    if value.is_null() {
        return None;
    }

    let bytes: *const libc::c_char = unsafe { msg_send![value, UTF8String] };
    if bytes.is_null() {
        return None;
    }

    Some(
        unsafe { CStr::from_ptr(bytes) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn app_matches_target(app: &RunningApp, target: &AppTarget) -> bool {
    match target {
        AppTarget::BundleId(id) => app.bundle_id.as_deref() == Some(id.as_str()),
        AppTarget::AppName(name) => {
            app.localized_name.as_deref() == Some(name.as_str())
                || app
                    .bundle_path
                    .as_deref()
                    .and_then(MacAppStateProbe::app_name_from_path)
                    .is_some_and(|app_name| app_name == name)
        }
        AppTarget::AppPath(path) => {
            let expanded = expand_tilde_path(path);
            app.bundle_path.as_deref() == Some(expanded.as_str())
        }
    }
}

fn expand_tilde_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|home| format!("{home}/{rest}"))
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

impl WindowCycler for MacWindowCycler {
    fn cycle_window(&self, target: &AppTarget, pid: i32) -> Result<(), ControllerError> {
        let app = AXUIElement::application(pid);
        app.set_messaging_timeout(1.0).map_err(map_ax_error)?;

        let windows = ax_windows(&app)?;
        let snapshots = snapshots_for_windows(&windows)?;
        let total_windows = snapshots.len();

        if total_windows == 0 {
            return Err(ControllerError::NoWindows {
                target: target_display(target),
            });
        }

        let cyclable = cyclable_windows(snapshots);
        if cyclable.is_empty() {
            return Err(ControllerError::NoCyclableWindows {
                target: target_display(target),
                total_windows,
                rejected_windows: total_windows,
            });
        }
        if cyclable.len() == 1 {
            return Err(ControllerError::OnlyOneCyclableWindow {
                target: target_display(target),
                title: cyclable[0].title.clone(),
            });
        }

        let focused_identity = current_window_identity(&app)?;
        let current_idx = focused_identity
            .and_then(|identity| cyclable.iter().position(|w| w.identity == identity))
            .or_else(|| cyclable.iter().position(|w| w.is_main))
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % cyclable.len();
        let next = &cyclable[next_idx];

        next.element.raise().map_err(map_ax_error)?;
        let _ = next.element.set_main(true);

        if verify_current_window(&app, &next.identity)? {
            Ok(())
        } else {
            Err(ControllerError::RaiseVerificationFailed {
                target: target_display(target),
                title: next.title.clone(),
            })
        }
    }
}

#[derive(Clone, Debug)]
struct WindowSnapshot {
    element: AXUIElement,
    identity: WindowIdentity,
    title: Option<String>,
    role: Option<String>,
    minimized: bool,
    size: Option<WindowSize>,
    is_main: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowIdentity {
    title: Option<String>,
    role: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowSize {
    width: i64,
    height: i64,
}

impl WindowSnapshot {
    fn is_cyclable(&self) -> bool {
        self.role.as_deref() == Some("AXWindow")
            && !self.minimized
            && self
                .size
                .is_none_or(|size| size.width > 0 && size.height > 0)
    }
}

fn ax_windows(app: &AXUIElement) -> Result<Vec<AXUIElement>, ControllerError> {
    app.windows()
        .map(|windows| windows.iter().map(|window| window.clone()).collect())
        .map_err(map_ax_error)
}

fn snapshots_for_windows(windows: &[AXUIElement]) -> Result<Vec<WindowSnapshot>, ControllerError> {
    windows
        .iter()
        .map(snapshot_for_window)
        .collect::<Result<Vec<_>, _>>()
}

fn snapshot_for_window(window: &AXUIElement) -> Result<WindowSnapshot, ControllerError> {
    let title = optional_ax(window.title()).map(|title| title.map(|value| value.to_string()))?;
    let role = optional_ax(window.role()).map(|role| role.map(|value| value.to_string()))?;
    let minimized = optional_ax(window.minimized())?
        .map(bool::from)
        .unwrap_or(false);
    let size = None;
    let is_main = optional_ax(window.main())?.map(bool::from).unwrap_or(false);
    let identity = WindowIdentity {
        title: title.clone(),
        role: role.clone(),
    };

    Ok(WindowSnapshot {
        element: window.clone(),
        identity,
        title,
        role,
        minimized,
        size,
        is_main,
    })
}

fn cyclable_windows(windows: Vec<WindowSnapshot>) -> Vec<WindowSnapshot> {
    let mut windows: Vec<_> = windows
        .into_iter()
        .filter(WindowSnapshot::is_cyclable)
        .collect();
    windows.sort_by_key(window_sort_key);
    windows
}

fn window_sort_key(window: &WindowSnapshot) -> String {
    window
        .title
        .as_deref()
        .map(str::to_lowercase)
        .unwrap_or_default()
}

fn current_window_identity(app: &AXUIElement) -> Result<Option<WindowIdentity>, ControllerError> {
    if let Some(focused) = optional_ax(app.focused_window())? {
        return snapshot_for_window(&focused).map(|snapshot| Some(snapshot.identity));
    }
    if let Some(main) = optional_ax(app.main_window())? {
        return snapshot_for_window(&main).map(|snapshot| Some(snapshot.identity));
    }
    Ok(None)
}

fn verify_current_window(
    app: &AXUIElement,
    expected: &WindowIdentity,
) -> Result<bool, ControllerError> {
    current_window_identity(app).map(|current| {
        // Some apps, notably Finder, allow AXRaise but do not expose
        // AXFocusedWindow/AXMainWindow consistently. In that case AXRaise is
        // the strongest available signal and verification is unsupported.
        current
            .as_ref()
            .map(|identity| identity == expected)
            .unwrap_or(true)
    })
}

fn optional_ax<T>(result: Result<T, AXError>) -> Result<Option<T>, ControllerError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(err) if is_ax_no_value(&err) => Ok(None),
        Err(err) => Err(map_ax_error(err)),
    }
}

fn is_ax_no_value(error: &AXError) -> bool {
    error.to_string().contains("kAXErrorNoValue")
}

fn map_ax_error(error: AXError) -> ControllerError {
    let message = error.to_string();
    if message.contains("APIDisabled") || message.contains("API disabled") {
        accessibility_permission_error()
    } else {
        ControllerError::AxApi(message)
    }
}

fn accessibility_permission_error() -> ControllerError {
    let executable = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let parent = parent_process_info();
    let likely_process = parent
        .as_ref()
        .map(|info| info.name.clone())
        .unwrap_or_else(|| "the terminal, launcher, or hotkey daemon that invokes summon".into());

    ControllerError::PermissionDenied {
        executable,
        parent: parent
            .map(|info| format!("{} (pid {})", info.name, info.pid))
            .unwrap_or_else(|| "unknown".to_string()),
        likely_process,
    }
}

/// Process details for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInfo {
    /// Process identifier.
    pub pid: i32,
    /// Process name or command.
    pub name: String,
}

/// Returns the current process details.
pub fn current_process_info() -> Option<ProcessInfo> {
    process_info(std::process::id() as i32)
}

/// Returns the parent process details.
pub fn parent_process_info() -> Option<ProcessInfo> {
    // SAFETY: `getppid` has no preconditions and does not dereference pointers.
    let pid = unsafe { libc::getppid() };
    process_info(pid)
}

/// Returns the grandparent process details.
pub fn grandparent_process_info() -> Option<ProcessInfo> {
    let parent = parent_process_info()?;
    parent_pid(parent.pid).and_then(process_info)
}

fn parent_pid(pid: i32) -> Option<i32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .ok()
}

fn process_info(pid: i32) -> Option<ProcessInfo> {
    let output = std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(ProcessInfo { pid, name })
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

    #[test]
    fn not_running_with_launch_returns_launch() {
        let target = finder();
        let controller = FakeAppController::new();
        let (action, context) = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Launch);
        assert_eq!(context.frontmost, ObservedFrontmost::NotChecked);
        assert!(context.launch_when_missing);
    }

    #[test]
    fn not_running_without_launch_returns_noop() {
        let target = finder();
        let controller = FakeAppController::new();
        let settings = EffectiveSettings {
            launch_if_not_running: false,
            ..EffectiveSettings::default()
        };
        let (action, context) = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::LaunchDisabled);
        assert_eq!(context.frontmost, ObservedFrontmost::NotChecked);
        assert!(!context.launch_when_missing);
    }

    // -- decide_action: running but not frontmost ----------------------------

    #[test]
    fn running_not_frontmost_returns_focus() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        let (action, context) = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Focus);
        assert_eq!(context.frontmost, ObservedFrontmost::No);
    }

    #[test]
    fn running_not_frontmost_ignores_cycle_setting() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        // Even with cycling enabled, a non-frontmost running app should focus.
        let (action, context) = decide_action(&controller, &target, &cycle_settings());
        assert_eq!(action, AppAction::Focus);
        assert!(context.cycle_when_focused);
    }

    // -- decide_action: running and frontmost ---------------------------------

    #[test]
    fn frontmost_with_cycle_returns_cycle() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let (action, context) = decide_action(&controller, &target, &cycle_settings());
        assert_eq!(action, AppAction::Cycle);
        assert_eq!(context.frontmost, ObservedFrontmost::Yes);
    }

    #[test]
    fn frontmost_without_cycle_returns_already_focused() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let (action, context) = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::AlreadyFocused);
        assert!(!context.cycle_when_focused);
    }

    // -- decide_action: all defaults (nothing enabled) ------------------------

    #[test]
    fn all_defaults_not_running_is_noop() {
        let target = finder();
        let controller = FakeAppController::new();
        let settings = EffectiveSettings::default();
        let (action, context) = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::LaunchDisabled);
        assert!(!context.is_running);
    }

    #[test]
    fn all_defaults_running_not_frontmost_is_focus() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, false);
        let settings = EffectiveSettings::default();
        let (action, context) = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::Focus);
        assert!(context.is_running);
    }

    #[test]
    fn all_defaults_frontmost_is_already_focused() {
        let target = finder();
        let controller = FakeAppController::new()
            .set_running(&target, true)
            .set_frontmost(&target, true);
        let settings = EffectiveSettings::default();
        let (action, context) = decide_action(&controller, &target, &settings);
        assert_eq!(action, AppAction::AlreadyFocused);
        assert_eq!(context.frontmost, ObservedFrontmost::Yes);
    }

    // -- decide_action: different target types --------------------------------

    #[test]
    fn decide_with_app_name_target() {
        let target = AppTarget::AppName("Preview".into());
        let controller = FakeAppController::new();
        let (action, _context) = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Launch);
    }

    #[test]
    fn decide_with_app_path_target() {
        let target = AppTarget::AppPath("/Applications/My App.app".into());
        let controller = FakeAppController::new().set_running(&target, true);
        let (action, _context) = decide_action(&controller, &target, &launch_settings());
        assert_eq!(action, AppAction::Focus);
    }

    // -- execute_action -------------------------------------------------------

    #[test]
    fn execute_noop_succeeds() {
        let target = finder();
        let controller = FakeAppController::new();
        let result = execute_action(&controller, &target, AppAction::AlreadyFocused);
        assert!(result.is_ok());

        let result = execute_action(&controller, &target, AppAction::LaunchDisabled);
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
            super::MacAppStateProbe::app_name_from_path("/Applications/Safari.app"),
            Some("Safari")
        );
    }

    #[test]
    fn app_name_from_path_with_spaces() {
        assert_eq!(
            super::MacAppStateProbe::app_name_from_path("/Applications/Visual Studio Code.app"),
            Some("Visual Studio Code")
        );
    }

    #[test]
    fn app_name_from_path_nested() {
        assert_eq!(
            super::MacAppStateProbe::app_name_from_path("/Applications/Utilities/Terminal.app"),
            Some("Terminal")
        );
    }

    #[test]
    fn app_name_from_path_tilde() {
        assert_eq!(
            super::MacAppStateProbe::app_name_from_path("~/Applications/My App.app"),
            Some("My App")
        );
    }

    #[test]
    fn app_name_from_path_no_app_extension() {
        assert_eq!(
            super::MacAppStateProbe::app_name_from_path("/Applications/Safari"),
            None
        );
    }

    #[test]
    fn app_name_from_path_empty() {
        assert_eq!(super::MacAppStateProbe::app_name_from_path(""), None);
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

    // This test calls real macOS Accessibility APIs and depends on local
    // Accessibility permission state.
    // Run manually with: cargo test -p summon --lib -- --ignored mac_controller_cycle
    #[test]
    #[ignore]
    fn mac_controller_cycle_runs_without_panic() {
        // Finder is always running on macOS. The cycle may succeed, fail with
        // an accessibility error depending on the test environment. We verify
        // only that the method runs without panicking and returns a well-typed result.
        let controller = super::MacAppController::new();
        let target = finder();
        let _result = controller.cycle_window(&target);
    }

    // -- app_matches_target ---------------------------------------------------

    #[test]
    fn app_matches_target_bundle_id() {
        let app = fake_running_app(
            Some("com.apple.finder"),
            Some("Finder"),
            Some("/System/Library/CoreServices/Finder.app"),
        );
        assert!(app_matches_target(
            &app,
            &AppTarget::BundleId("com.apple.finder".into())
        ));
    }

    #[test]
    fn app_matches_target_app_name() {
        let app = fake_running_app(
            Some("com.apple.Preview"),
            Some("Preview"),
            Some("/System/Applications/Preview.app"),
        );
        assert!(app_matches_target(
            &app,
            &AppTarget::AppName("Preview".into())
        ));
    }

    #[test]
    fn app_matches_target_app_path_with_spaces() {
        let app = fake_running_app(
            Some("com.microsoft.VSCode"),
            Some("Visual Studio Code"),
            Some("/Applications/Visual Studio Code.app"),
        );
        assert!(app_matches_target(
            &app,
            &AppTarget::AppPath("/Applications/Visual Studio Code.app".into())
        ));
    }

    // -- window filtering -----------------------------------------------------

    #[test]
    fn window_snapshot_filters_minimized_windows() {
        let window = fake_window_snapshot(Some("AXWindow"), true, Some((0, 0)), Some((100, 100)));
        assert!(!window.is_cyclable());
    }

    #[test]
    fn window_snapshot_filters_non_window_roles() {
        let window = fake_window_snapshot(Some("AXSheet"), false, Some((0, 0)), Some((100, 100)));
        assert!(!window.is_cyclable());
    }

    #[test]
    fn window_snapshot_filters_zero_size_windows() {
        let window = fake_window_snapshot(Some("AXWindow"), false, Some((0, 0)), Some((0, 100)));
        assert!(!window.is_cyclable());
    }

    #[test]
    fn window_snapshot_accepts_normal_window() {
        let window = fake_window_snapshot(Some("AXWindow"), false, Some((0, 0)), Some((100, 100)));
        assert!(window.is_cyclable());
    }

    fn fake_window_snapshot(
        role: Option<&str>,
        minimized: bool,
        _position: Option<(i64, i64)>,
        size: Option<(i64, i64)>,
    ) -> WindowSnapshot {
        let size = size.map(|(width, height)| WindowSize { width, height });
        WindowSnapshot {
            element: AXUIElement::system_wide(),
            identity: WindowIdentity {
                title: Some("Test".into()),
                role: role.map(str::to_string),
            },
            title: Some("Test".into()),
            role: role.map(str::to_string),
            minimized,
            size,
            is_main: false,
        }
    }

    fn fake_running_app(
        bundle_id: Option<&str>,
        localized_name: Option<&str>,
        bundle_path: Option<&str>,
    ) -> RunningApp {
        RunningApp {
            pid: 123,
            bundle_id: bundle_id.map(str::to_string),
            localized_name: localized_name.map(str::to_string),
            bundle_path: bundle_path.map(str::to_string),
        }
    }
}
