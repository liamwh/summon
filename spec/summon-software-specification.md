# Summon

Summon is a tiny macOS command-line tool for opening, focusing, and cycling applications from declarative keybindings.

Summon gives macOS users a fast, reliable way to jump to the applications they use most. It is designed for keyboard-driven workflows, launcher integrations, tiling window managers, and developers who want a simple, scriptable alternative to heavyweight automation tools.

At its core, Summon does one thing well:

> Press a keybinding, and the app you want appears.

If the app is not running, Summon opens it.  
If the app is already running, Summon focuses it.  
If the currently focused window already belongs to that app, Summon can cycle to the next matching window.

## Why Summon exists

macOS has excellent applications, but switching between them precisely is still more awkward than it should be.

The Dock is mouse-oriented.  
Spotlight launches apps but does not behave like a deterministic app switcher.  
Mission Control is visual, not scriptable.  
Existing automation tools are often powerful but heavy, opaque, or difficult to version-control.

Summon is for people who want their workspace to feel programmable.

It provides a small, predictable command-line interface backed by a simple configuration file. Your app-switching workflow becomes declarative, portable, and easy to keep in dotfiles.

## Product principles

### 1. Small by default

Summon should be a small binary with a small surface area. It should do app launching and focusing well, without trying to become a full window manager, launcher, automation platform, or productivity suite.

### 2. Configuration as code

User configuration should live in a TOML file under the XDG config directory. It should be readable, version-controllable, and easy to manage with tools like stow, chezmoi, Nix, or a plain Git repository.

### 3. Native macOS behaviour

Summon should feel like it belongs on macOS. It should use bundle identifiers where possible, support application names where convenient, and integrate cleanly with macOS Accessibility permissions.

### 4. Fast enough to disappear

The common path should be effectively instant. Summon should add as little latency as possible between a keypress and the desired application being frontmost.

### 5. Composable, not controlling

Summon should not require users to adopt a specific hotkey daemon or launcher. It should work well with skhd, Raycast, Alfred, Karabiner-Elements, Hammerspoon, shell aliases, and any other tool capable of executing a command.

## Target users

Summon is built for:

- Developers using keyboard-driven workflows on macOS.
- Users who manage their configuration through dotfiles.
- People who want Linux-style app focusing behaviour on macOS.
- Users of tiling window managers such as AeroSpace, yabai, or Amethyst.
- Power users who prefer simple command-line tools over graphical preference panes.

## Non-goals

Summon is not intended to be:

- A full window manager.
- A replacement for Spotlight, Raycast, or Alfred.
- A global hotkey daemon.
- A graphical app launcher.
- A general-purpose macOS automation framework.
- A menu bar utility.
- A complex rules engine for window placement.

Summon may integrate with these tools, but it should not become them.

## Core behaviour

Summon provides commands for launching, focusing, and cycling macOS applications.

The basic behaviour is:

1. Resolve the configured target application.
2. Check whether the application is currently running.
3. If it is not running, launch it.
4. If it is running, focus its most recently used window.
5. If the currently focused window already belongs to the target application, optionally cycle to the next matching window.

This makes repeated keypresses useful rather than redundant.

For example:

sh summon app terminal 

Could mean:

- Open Ghostty if it is not running.
- Focus Ghostty if it is running but not focused.
- Cycle to the next Ghostty window if Ghostty is already focused.

## Command-line interface

The command-line interface should be minimal and stable.

### Primary command

sh summon <binding> 

Runs the configured binding with the given name.

Example:

sh summon terminal summon browser summon editor summon notes 

### Direct app command

sh summon app <app> 

Summons an application directly by name or bundle identifier.

Examples:

sh summon app Ghostty summon app com.mitchellh.ghostty summon app "Visual Studio Code" 

### Listing configured bindings

sh summon list 

Prints all configured bindings.

Example output:

text terminal  -> com.mitchellh.ghostty browser   -> com.brave.Browser editor    -> com.microsoft.VSCode notes     -> md.obsidian 

### Configuration path

sh summon config path 

Prints the active configuration file path.

Example:

text /Users/liam/.config/summon/summon.toml 

### Configuration validation

sh summon config check 

Validates the configuration file and prints any errors.

### Permission check

sh summon doctor 

Checks whether Summon has the macOS permissions it needs.

The first version should check for:

- Accessibility permission.
- Whether target applications can be resolved.
- Whether configured bundle identifiers appear valid.
- Whether the configuration file can be read.

## Configuration

Summon uses a TOML configuration file.

Default path:

text $XDG_CONFIG_HOME/summon/summon.toml 

If XDG_CONFIG_HOME is not set, Summon should fall back to:

text ~/.config/summon/summon.toml 

Although macOS traditionally uses ~/Library/Application Support, Summon intentionally supports the XDG config layout because it is designed for dotfile-driven workflows.

## Example configuration

toml # ~/.config/summon/summon.toml  [settings] cycle_when_focused = true launch_if_not_running = true focus_strategy = "recent-window"  [bindings.terminal] app = "com.mitchellh.ghostty"  [bindings.browser] app = "com.brave.Browser"  [bindings.editor] app = "com.microsoft.VSCode"  [bindings.notes] app = "md.obsidian"  [bindings.chat] app = "com.tinyspeck.slackmacgap"  [bindings.music] app = "com.spotify.client" 

## Binding model

Each binding maps a human-friendly name to an application target.

Minimum binding:

toml [bindings.terminal] app = "com.mitchellh.ghostty" 

Expanded binding:

toml [bindings.terminal] app = "com.mitchellh.ghostty" cycle_when_focused = true launch_if_not_running = true focus_strategy = "recent-window" 

Per-binding settings should override global settings.

## Application resolution

Summon should support application resolution by:

1. Bundle identifier.
2. Exact application name.
3. Application path.

Bundle identifiers should be preferred because they are stable and unambiguous.

Examples:

toml [bindings.terminal] app = "com.mitchellh.ghostty"  [bindings.preview] app = "Preview"  [bindings.custom] app = "/Applications/My Custom App.app" 

When the target is ambiguous, Summon should return a clear error instead of guessing silently.

## Focus strategies

Initial supported focus strategy:

toml focus_strategy = "recent-window" 

This should focus the most recently used window belonging to the target application.

Future strategies may include:

toml focus_strategy = "first-window" focus_strategy = "largest-window" focus_strategy = "visible-window" focus_strategy = "next-window" 

The first release does not need all of these. The configuration should leave room for them.

## Cycling behaviour

Cycling is one of Summon’s most important behaviours.

When cycle_when_focused is enabled:

- If the target app is not focused, Summon focuses it.
- If the target app is already focused, Summon focuses the next window belonging to that app.
- Repeated invocations continue cycling through that app’s windows.

This is especially useful for browsers, terminals, editors, and chat applications.

Example:

sh summon terminal summon terminal summon terminal 

Could cycle through multiple Ghostty windows.

## Suggested integration with skhd

Summon should not own global keybindings itself in the first version. Instead, it should document integrations with existing tools.

Example skhd configuration:

text cmd - return : summon terminal cmd - b      : summon browser cmd - e      : summon editor cmd - n      : summon notes cmd - m      : summon music 

This keeps Summon small and composable.

## Suggested integration with Raycast

Users should be able to create Raycast script commands that call Summon.

Example:

sh #!/bin/bash # @raycast.schemaVersion 1 # @raycast.title Summon Terminal # @raycast.mode silent  summon terminal 

## Suggested integration with shell aliases

sh alias st='summon terminal' alias sb='summon browser' alias se='summon editor' 

## Rust implementation

Summon should be implemented in Rust.

Recommended crate choices:

- clap for command-line parsing.
- serde for configuration deserialisation.
- toml for reading TOML configuration.
- thiserror for typed errors.
- miette or color-eyre for good diagnostic output.
- tracing for structured logging.
- directories or xdg for config path resolution.
- objc2, accessibility, or shell-backed macOS automation for native interaction.

The first implementation may use macOS command-line primitives where appropriate, but the long-term direction should favour native APIs for correctness and performance.

## Proposed internal architecture

text summon ├── cli │   ├── parse arguments │   └── dispatch commands ├── config │   ├── resolve config path │   ├── read TOML │   └── validate bindings ├── app │   ├── resolve app target │   ├── detect running apps │   ├── launch app │   └── focus app/window ├── window │   ├── inspect frontmost app │   ├── list windows for app │   └── cycle windows └── diagnostics     ├── permissions     ├── config health     └── app resolution checks 

## Error handling

Summon should fail loudly and usefully.

Bad:

text Error 

Good:

text Could not resolve binding: terminal  No binding named "terminal" was found in:   /Users/liam/.config/summon/summon.toml  Available bindings:   browser   editor   notes 

Bad:

text Permission denied 

Good:

text Summon needs Accessibility permission to focus application windows.  Open:   System Settings → Privacy & Security → Accessibility  Then enable Summon for your terminal, launcher, or hotkey daemon. 

## Configuration validation rules

Summon should validate that:

- The config file is valid TOML.
- Binding names are non-empty.
- Each binding has an app field.
- focus_strategy is one of the supported values.
- Boolean fields are valid booleans.
- Application paths exist if a path is provided.
- Bundle identifiers are syntactically plausible.
- Per-binding settings do not use unknown fields.

Unknown fields should produce a warning or error. For a small tool, strict validation is preferable.

## Installation

Preferred installation methods:

sh cargo install summon 

Or via Homebrew:

sh brew install liamwh/tap/summon 

Manual installation should also be supported:

sh curl -L https://github.com/liamwh/summon/releases/latest/download/summon-aarch64-apple-darwin.tar.gz \   | tar xz  sudo mv summon /usr/local/bin/summon 

## Repository layout

text summon ├── Cargo.toml ├── README.md ├── LICENSE ├── crates │   └── summon │       ├── src │       │   ├── main.rs │       │   ├── cli.rs │       │   ├── config.rs │       │   ├── app.rs │       │   ├── window.rs │       │   ├── diagnostics.rs │       │   └── error.rs │       └── tests ├── examples │   ├── summon.toml │   ├── skhdrc │   └── raycast └── docs     ├── configuration.md     ├── permissions.md     └── integrations.md 

## Minimum viable product

The first release should include:

- summon <binding>
- summon app <app>
- summon list
- summon config path
- summon config check
- summon doctor
- TOML configuration under XDG config home.
- App resolution by bundle identifier.
- Launch app if not running.
- Focus app if running.
- Basic cycle support when the target app is already focused.
- Clear errors for missing config, missing bindings, and permission issues.
- Documentation for skhd, Raycast, and shell alias usage.

## Later features

Potential future features:

- Built-in keybinding daemon.
- Native background service.
- Menu bar companion app.
- Per-display focus rules.
- Per-space focus rules.
- Window title matching.
- Browser profile support.
- App groups.
- Fallback chains.
- Recent app stack.
- JSON output for scripting.
- Shell completions.
- Nix package.
- Homebrew tap.
- Config initialisation wizard.
- Import from existing skhd config.
- Integration with AeroSpace, yabai, or Amethyst.

## Example advanced configuration

toml [settings] cycle_when_focused = true launch_if_not_running = true focus_strategy = "recent-window"  [bindings.terminal] app = "com.mitchellh.ghostty" cycle_when_focused = true  [bindings.browser] app = "com.brave.Browser" cycle_when_focused = true  [bindings.editor] app = "com.microsoft.VSCode" cycle_when_focused = false  [bindings.notes] app = "md.obsidian"  [bindings.calendar] app = "com.apple.iCal"  [bindings.mail] app = "com.apple.mail"  [bindings.design] app = "com.figma.Desktop"  [bindings.passwords] app = "com.1password.1password" 

## Positioning

Summon is not a launcher.

It is a deterministic app focus tool for macOS.

Launchers help you find things.  
Summon brings the thing you already chose to the front.

## Tagline options

Summon — call any macOS app to the front.

Summon — open, focus, and cycle macOS apps from your keyboard.

Summon — deterministic app switching for macOS.

Summon — a tiny command-line app focuser for macOS.

## One-line description

Summon is a small Rust command-line tool for opening, focusing, and cycling macOS applications from a TOML config file.

## README introduction

Summon is a tiny macOS command-line tool for keyboard-driven app switching.

Define your favourite applications once in ~/.config/summon/summon.toml, wire them to your preferred hotkey tool, and use one command to launch, focus, or cycle through app windows.

sh summon terminal summon browser summon editor 

Summon is designed to be fast, boring, and easy to keep in your dotfiles.
