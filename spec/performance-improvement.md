# Implement Native macOS Window Cycling + Fast Hot Path + `summond` Daemon

## Your role

You are an autonomous coding agent working on the `summon` Rust project.

Your task is to implement the perfect version of the window-cycling fix and performance architecture.

Do not implement a minimal patch. Do not preserve AppleScript in the hot path. The goal is a robust, native, fast macOS implementation with typed errors, useful diagnostics, verbose observability, and an optional daemon mode for maximum snappiness.

## Problem

When a user triggers:

summon zed

while Zed is already frontmost, nothing happens.

The existing decision logic correctly routes to `Cycle`, but the execution is fragile because cycling currently uses AppleScript:

set index of window 2 to 1

This is unreliable across macOS apps and window types. It is also slower than necessary because the hot path currently relies on repeated AppleScript calls for app state queries and PID lookup.

## Desired final architecture

Replace the AppleScript-based hot path with native macOS APIs.

The final architecture should be:

1. Native app observation layer
   - Use native macOS APIs, preferably `NSWorkspace`, CoreGraphics, or safe Rust bindings around them.
   - Detect whether the target app is running.
   - Detect whether the target app is frontmost.
   - Resolve PID.
   - Prefer bundle identifier matching over app-name matching.
   - Return all observed state in one call.

2. Native window manipulation layer
   - Use native macOS Accessibility API via `AXUIElement`.
   - Enumerate windows.
   - Filter to cyclable windows.
   - Determine the current window using app-level `AXFocusedWindow` first, then `AXMainWindow`, then a sensible fallback.
   - Select the next cyclable window deterministically.
   - Raise it using `AXRaise`.
   - Best-effort set focus if supported.
   - Verify that the focused/main window changed.
   - Return typed success/failure outcomes.

3. AppleScript fallback layer
   - AppleScript must not be used on the normal hot path.
   - Keep AppleScript only as a compatibility fallback if absolutely necessary.
   - Any fallback usage must be explicit in verbose output.

4. Optional daemon mode
   - Add a long-running daemon called `summond`.
   - The daemon keeps config loaded, watches for config changes, and performs native app observation and native window manipulation.
   - The `summon` CLI should be able to send commands to the daemon over a Unix domain socket.
   - If the daemon is unavailable, the CLI may fall back to direct mode unless configured otherwise.
   - The daemon should give the fastest possible keybinding/Raycast path.
   - The daemon also gives users one stable process to grant macOS Accessibility permission to.

## Performance goals

Make the command feel instant.

Target behaviour:

- Direct native mode should avoid all avoidable process spawning.
- Daemon mode should avoid repeated config parsing, app target resolution, framework initialisation, and AppleScript startup overhead.
- Verbose and diagnostic modes may collect richer data, but default hot-path execution should collect only what it needs.

Avoid doing this on the default hot path:

- Fetching every window title.
- Fetching every window position/size.
- Building detailed debug reports.
- Running AppleScript.
- Spawning `osascript`.
- Performing duplicate app/PID lookups.

## Files likely to modify

Inspect the repository first and adapt this list if the actual structure differs.

Likely files:

- `crates/summon/Cargo.toml`
- `crates/summon/src/controller.rs`
- `crates/summon/src/cli.rs`
- `crates/summon/src/diagnostics.rs`
- any existing config-loading module
- any existing command dispatch module
- tests near the above files

You may add new modules if that gives a cleaner architecture.

Suggested new modules:

- `crates/summon/src/macos/mod.rs`
- `crates/summon/src/macos/app_observer.rs`
- `crates/summon/src/macos/window_cycler.rs`
- `crates/summon/src/macos/accessibility.rs`
- `crates/summon/src/daemon/mod.rs`
- `crates/summon/src/daemon/client.rs`
- `crates/summon/src/daemon/server.rs`
- `crates/summon/src/daemon/protocol.rs`

## Dependency guidance

Before adding dependencies, inspect available crate APIs and choose the most maintainable option.

Candidates to evaluate:

- `axuielement`
- `accessibility`
- `accessibility-sys`
- `core-foundation`
- `core-graphics`
- `cocoa`
- `objc2` / modern Objective-C bindings if already used or clearly better
- `notify` for config watching
- `tokio` Unix socket support if Tokio is already in the project
- `serde` / `serde_json` or a compact binary format for daemon protocol

Do not blindly assume a crate exposes methods like `app.windows()` or `window.focused()`. Verify the actual API and build a small internal wrapper if needed.

Prefer a small internal wrapper over `AXUIElement` so the rest of the code is not tightly coupled to a third-party crate’s exact convenience API.

## Typed errors

Replace stringly typed controller errors with a typed error enum.

Add or adapt something like:

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error("Could not launch {target}: {reason}")]
    LaunchFailed {
        target: String,
        reason: String,
    },

    #[error("Could not focus {target}: {reason}")]
    FocusFailed {
        target: String,
        reason: String,
    },

    #[error("Accessibility permission denied for the current process")]
    AccessibilityDenied,

    #[error("Accessibility permission denied for the current process; likely launcher: {launcher}")]
    AccessibilityDeniedLikelyLauncher {
        launcher: String,
    },

    #[error("Could not find a running process for {target}")]
    AppNotRunning {
        target: String,
    },

    #[error("Could not resolve PID for {target}")]
    PidLookupFailed {
        target: String,
    },

    #[error("{target} has no windows")]
    NoWindows {
        target: String,
    },

    #[error("{target} has only one cyclable window")]
    OnlyOneCyclableWindow {
        target: String,
        title: Option<String>,
    },

    #[error("{target} has no cyclable windows; total windows: {total_windows}, rejected windows: {rejected_windows}")]
    NoCyclableWindows {
        target: String,
        total_windows: usize,
        rejected_windows: usize,
    },

    #[error("Raised a window for {target}, but macOS did not report it as focused afterwards")]
    RaiseVerificationFailed {
        target: String,
    },

    #[error("Accessibility API error: {0}")]
    AxApi(String),

    #[error("macOS API error: {0}")]
    MacOsApi(String),

    #[error("Daemon error: {0}")]
    Daemon(String),
}

Use more precise variants if the existing codebase suggests better names.

Avoid returning `Result<_, String>` from controller logic.

## App action model

Update `AppAction` to make no-op states explicit:

pub enum AppAction {
    Launch,
    Focus,
    Cycle,
    AlreadyFocused,
    LaunchDisabled,
}

Do not use a generic `NoOp` if a more precise variant is possible.

## App observation model

Replace separate `is_running`, `is_frontmost`, and `pid_for_target` hot-path calls with a single native observation call.

Add something like:

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedFrontmost {
    Yes,
    No,
    NotChecked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppObservation {
    pub is_running: bool,
    pub frontmost: ObservedFrontmost,
    pub pid: Option<i32>,
    pub bundle_id: Option<String>,
    pub app_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionContext {
    pub is_running: bool,
    pub frontmost: ObservedFrontmost,
    pub launch_when_missing: bool,
    pub cycle_when_focused: bool,
}

pub trait AppStateProbe {
    fn observe(&self, target: &AppTarget) -> Result<AppObservation, ControllerError>;
}

`decide_action` should become pure or nearly pure:

pub fn decide_action_from_observation(
    observation: &AppObservation,
    settings: &AppSettings,
) -> (AppAction, DecisionContext)

The decision logic should be:

- not running + launch enabled => `Launch`
- not running + launch disabled => `LaunchDisabled`
- running + not frontmost => `Focus`
- running + frontmost + cycle enabled => `Cycle`
- running + frontmost + cycle disabled => `AlreadyFocused`

When the app is not running, `frontmost` should be `ObservedFrontmost::NotChecked`.

## Native app observation

Implement a native macOS observer.

It should:

- Use bundle identifier where available.
- Fall back to app name only where necessary.
- Return PID in the same observation.
- Determine frontmost status using the native frontmost app, not AppleScript.
- Prefer exact bundle ID match.
- If multiple processes match, prefer the frontmost matching process.
- If no frontmost match exists, use a stable deterministic fallback.

Do not call AppleScript on the normal path.

## Native window cycling

Implement a native `WindowCycler`.

Suggested interface:

pub trait WindowCycler {
    fn cycle_window(
        &self,
        target: &AppTarget,
        observation: &AppObservation,
        verbosity: Verbosity,
    ) -> Result<CycleOutcome, ControllerError>;
}

Suggested result:

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleOutcome {
    pub pid: i32,
    pub total_windows: usize,
    pub cyclable_windows: usize,
    pub previous_window_title: Option<String>,
    pub raised_window_title: Option<String>,
    pub verified: bool,
    pub used_fallback: bool,
}

In default mode, avoid fetching window titles unless cheap or already required. It is acceptable for titles to be `None` outside verbose mode.

Cycling algorithm:

1. Require `observation.pid`.
2. Create `AXUIElement` application for the PID.
3. Read app windows.
4. Filter windows to cyclable windows.
5. Determine current window:
   - Prefer app-level `AXFocusedWindow`.
   - Else app-level `AXMainWindow`.
   - Else first cyclable window.
6. Select the next cyclable window:
   - Find current in the filtered list.
   - Next index is `(current_index + 1) % cyclable_windows.len()`.
7. Perform `AXRaise` on the selected window.
8. Best-effort set focus if supported.
9. Re-read app-level focused/main window.
10. Verify the selected window became focused/main.
11. Return `CycleOutcome`.

Cyclable window filtering:

Include by default:

- Normal windows.
- Not minimised.
- Non-zero size if size is cheaply available.
- Windows belonging to the target app.

Usually exclude:

- Sheets.
- Tooltips.
- Popovers.
- Hidden utility panels.
- Zero-size windows.
- Non-window accessibility elements.

Be careful with apps that expose unusual window roles. Prefer robust behaviour and useful diagnostics over overly strict filtering.

If there are no windows, return `NoWindows`.

If there is exactly one cyclable window, return `OnlyOneCyclableWindow`.

If there are windows but none are cyclable, return `NoCyclableWindows`.

## Accessibility permission handling

Use native Accessibility trust checks.

Important: `AXIsProcessTrusted()` checks whether the current process is trusted. It does not directly check the parent process.

Diagnostics should therefore distinguish:

- current executable
- current PID
- parent process
- grandparent process if easy
- current process AX trust
- likely launcher that may need permissions

Do not confidently say “skhd needs permission” unless you have proof. Say “likely launcher” or “process to check”.

Add helper functions:

- `current_executable()`
- `parent_process_info()`
- `process_name(pid)`
- optionally `grandparent_process_info()`

For `summon doctor`, report:

- Current executable path
- Current PID
- Parent process name and PID
- Grandparent process name and PID if available
- `AXIsProcessTrusted` status
- Whether AX system-wide element can be queried
- Whether frontmost app can be queried
- Whether target app can be queried if a target is supplied
- Window enumeration status

Consider adding:

summon doctor --request-accessibility

Only this mode should request a macOS permission prompt if supported. Plain `summon doctor` should inspect and report without surprising the user with a prompt.

## Verbose output

Add global verbosity:

#[arg(short, long, global = true, action = clap::ArgAction::Count)]
pub verbose: u8

Represent it internally as:

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    Quiet,
    Decision,
    Debug,
}

`-v` should print decision and context to stderr.

Example:

summon zed: pid=12345 running=true frontmost=true cycle_when_focused=true -> Cycle

`-vv` should include backend details.

Example:

summon zed:
  target: Zed
  bundle_id: dev.zed.Zed
  pid: 12345
  running: true
  frontmost: true
  action: Cycle
  backend: native-macos
  windows: total=3 cyclable=2
  raised: "controller.rs — summon"
  verified: true

Do not trigger expensive debug collection unless `-vv` is enabled.

## Daemon mode

Add `summond`.

The daemon should:

- Load summon config once.
- Watch config files and reload on change.
- Listen on a Unix domain socket.
- Accept commands from the `summon` CLI.
- Execute native observation and actions.
- Return structured results.
- Use the same typed errors as direct mode, serialised over the socket.
- Shut down gracefully.
- Avoid AppleScript on its hot path.

Suggested CLI:

summon daemon run
summon daemon stop
summon daemon status

And normal command behaviour:

summon zed

should try to use the daemon if daemon mode is enabled or available, then fall back to direct native mode unless configured not to.

Add config if appropriate:

[daemon]
enabled = true
socket_path = "~/.summon/summond.sock"
fallback_to_direct = true

If the existing config style differs, adapt to match it.

The daemon protocol should be versioned.

Example request:

{
  "version": 1,
  "command": {
    "type": "RunTarget",
    "target": "zed",
    "verbosity": "Quiet"
  }
}

Example response:

{
  "version": 1,
  "result": {
    "action": "Cycle",
    "observation": {
      "is_running": true,
      "frontmost": "Yes",
      "pid": 12345,
      "bundle_id": "dev.zed.Zed",
      "app_name": "Zed"
    },
    "cycle_outcome": {
      "pid": 12345,
      "total_windows": 3,
      "cyclable_windows": 2,
      "verified": true,
      "used_fallback": false
    }
  }
}

Use JSON unless there is already a stronger convention in the repo. The protocol should be easy to inspect while debugging.

Daemon socket path requirements:

- Use a stable path.
- Ensure parent directory exists.
- Clean up stale socket files safely.
- Refuse to connect to obviously invalid sockets.
- Handle daemon-not-running cleanly.

## CLI execution model

Refactor command execution so direct mode and daemon mode share the same core logic.

Suggested structure:

- `CommandRunner` or equivalent application service.
- Direct runner uses native observer + native controller.
- Daemon client sends request and renders response.
- Daemon server receives request and calls same core runner.

Avoid duplicating decision logic between CLI and daemon.

## Launch and focus

Replace AppleScript for launch/focus where feasible.

For launching:

- Prefer native `NSWorkspace` launch/open APIs.
- Preserve existing behaviour for configured paths, bundle IDs, or app names.
- Use AppleScript only as fallback if native launch cannot support a current feature.

For focusing:

- Prefer native app activation APIs.
- Use process/app activation by PID or bundle ID.
- Avoid AppleScript on the normal path.
- Keep fallback explicit and visible in `-vv`.

## Diagnostics

Update `summon doctor`.

It should report:

- Config file status
- Native macOS backend availability
- Current executable path
- Current process PID
- Parent process
- Grandparent process if available
- Accessibility trust status for current process
- Whether AX system-wide query works
- Whether native frontmost app detection works
- Whether native running-app enumeration works
- Whether daemon is running
- Daemon socket path
- Daemon protocol version
- Config reload status if daemon is running

Support target-specific diagnostics:

summon doctor zed

This should report:

- target resolution
- bundle ID
- running status
- PID
- frontmost status
- AX app element access
- total windows
- cyclable windows
- current window if available
- next window if available
- whether cycling would work

Do not require two windows for generic `doctor` to pass. A target with only one window should report “only one cyclable window” as an informational state, not necessarily a failure.

## Tests

Update and expand tests.

Decision tests:

- not running + launch enabled => Launch
- not running + launch disabled => LaunchDisabled
- running + not frontmost + cycle enabled => Focus
- running + not frontmost + cycle disabled => Focus
- running + frontmost + cycle enabled => Cycle
- running + frontmost + cycle disabled => AlreadyFocused
- frontmost is NotChecked when app is not running
- DecisionContext contains settings-derived facts

CLI tests:

- `summon -v zed` parses verbosity 1
- `summon -vv zed` parses verbosity 2
- daemon subcommands parse correctly
- doctor target argument parses correctly if added

Window cycler unit tests with fake AX backend:

- no windows => NoWindows
- one normal window => OnlyOneCyclableWindow
- two normal windows, first focused => raises second
- two normal windows, second focused => raises first
- minimised windows are excluded
- sheets/dialogs/popovers are excluded unless intentionally supported
- no focused/main window => fallback is deterministic
- raise succeeds but verification fails => RaiseVerificationFailed
- permission denied maps to AccessibilityDenied

Daemon tests:

- protocol serialisation round trip
- client handles missing socket cleanly
- server handles one RunTarget request
- stale socket cleanup
- config reload event if feasible without flaky sleeps

macOS ignored integration tests:

- native observer can find a known running app
- native observer can detect frontmost app
- native AX trust check works
- native cycle test for a controllable app with two windows
- daemon can execute a target command over Unix socket

Use ignored integration tests for anything requiring real macOS windows or Accessibility permission.

## Benchmarking / performance instrumentation

Add lightweight timing instrumentation behind `-vv`.

For example:

summon zed:
  observe: 4.2ms
  decide: 0.1ms
  cycle: 7.8ms
  total: 14.6ms
  backend: native-macos

Do not always print timings. Only print with `-vv`.

If the repo already uses tracing, use spans. Otherwise, simple `Instant` measurements are fine.

Add at least one benchmark or test helper if the repo already has a benchmark setup. Do not add a heavy benchmark framework unless it fits the project.

## Implementation quality requirements

- Keep the hot path fast.
- Keep decision logic pure and well tested.
- Keep macOS-specific code isolated.
- Avoid stringly typed errors.
- Avoid duplicate PID/app lookups.
- Avoid AppleScript on the normal path.
- Avoid expensive debug data collection unless `-vv` or `doctor`.
- Prefer bundle IDs for app identity.
- Make daemon protocol versioned.
- Make daemon failure modes explicit and friendly.
- Keep fallback behaviour visible in verbose output.
- Preserve existing public behaviour unless it conflicts with the new correct design.

## Acceptance criteria

The implementation is complete when:

1. `cargo test -p summon` passes.
2. CLI parsing tests cover `-v`, `-vv`, daemon commands, and doctor target mode.
3. Decision tests cover the full state matrix.
4. Window cycling unit tests cover fake AX behaviours.
5. AppleScript is no longer used in the normal hot path for running/frontmost/PID/cycle/focus where native APIs can support the behaviour.
6. `summon -v zed` prints a clear decision line.
7. `summon -vv zed` prints timing and backend details.
8. `summon zed` with Zed already frontmost cycles to the next Zed window.
9. `summon zed` with Zed running but not frontmost focuses Zed.
10. `summon zed` with Zed not running launches Zed if launch is enabled.
11. `summon doctor` reports native backend and Accessibility status.
12. `summon doctor zed` reports target-specific app/window state.
13. `summon daemon run` starts a daemon and listens on a Unix socket.
14. `summon zed` can execute through the daemon.
15. If the daemon is unavailable, the CLI cleanly falls back to direct mode when configured to do so.
16. Permission failures explain the current process and likely launcher without making overconfident claims.
17. The default hot path avoids fetching expensive debug window metadata.
18. `-vv` includes enough detail to debug state, backend, timing, and window cycling.

## Important design note

Do not treat this as:

replace `set index of window 2 to 1` with `AXRaise`

Treat it as:

build a native, typed, fast macOS app-control backend around app observation and Accessibility window manipulation, then add a daemon so repeated invocations are warm and snappy.
