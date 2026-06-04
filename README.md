# Summon

Summon is a tiny macOS command-line tool for keyboard-driven app switching.

Define your favourite applications in `~/.config/summon/summon.toml`, wire them to your preferred hotkey tool, and use one command to launch, focus, or cycle through app windows.

```sh
summon terminal
summon browser
summon editor
```

Summon is designed to be fast, boring, and easy to keep in your dotfiles.

## Installation

```sh
cargo install summon
```

## Quick start

Create `~/.config/summon/summon.toml`:

```toml
[settings]
cycle_when_focused = true
launch_if_not_running = true

[bindings.terminal]
app = "com.mitchellh.ghostty"

[bindings.browser]
app = "com.brave.Browser"

[bindings.editor]
app = "dev.zed.Zed"
```

Wire to your hotkey tool (e.g. skhd):

```
cmd + alt + ctrl + shift - return : summon terminal
cmd + alt + ctrl + shift - b      : summon browser
cmd + alt + ctrl + shift - z      : summon editor
```

## Usage

```sh
summon <binding>        # Launch, focus, or cycle the configured app
summon app <app>        # Summon an app directly by name or bundle ID
summon list             # List all configured bindings
summon config path      # Print the active config file path
summon config check     # Validate the config file
summon doctor           # Check macOS permissions
```

## License

MIT
