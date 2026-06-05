# Summon implementation progress

## Last updated

- Commit: 27932ff (lib.rs + main.rs split)
- Date: 2026-06-05
- Agent task: Completion audit — fix stale comment in integration tests

## What changed in this iteration

- Fixed stale "Unimplemented commands" section header in `integration.rs`
  (all commands are implemented; header now reads accurately)

## Verification run

- `cargo test --workspace` — 122 library + 11 binary + 14 integration + 1 doc test = 148 passed
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo build --release` — builds 752 KB binary
- `summon --help` / `summon --version` — correct output
- No TODO/FIXME/todo!/unimplemented! markers found in Rust source

## Current state reconstructed from git

- Done: All v1 MVP spec items implemented and tested
  - `summon <binding>` — core command path (config → resolve → decide → execute)
  - `summon app <app>` — direct app targeting by bundle ID, name, or path
  - `summon list` — prints configured bindings
  - `summon config path` — prints active config path
  - `summon config check` — validates config file
  - `summon doctor` — checks config, bindings, and Accessibility permissions
  - XDG config path resolution with `~/.config` fallback
  - TOML config with strict validation (unknown fields, empty apps, invalid strategies)
  - Per-binding settings override global settings
  - `AppController` trait with `FakeAppController` (deterministic) and `MacAppController` (real macOS)
  - Launch via `open -b` / `open -a` / `open <path>`
  - Focus via `open` (brings app to foreground)
  - Window cycling via macOS Accessibility API (AppleScript)
  - Clear errors for missing config, missing bindings, invalid targets, permission issues
  - Example configs: summon.toml, skhdrc, Raycast scripts, shell aliases
  - CI pipeline (format, clippy, test)
  - Release workflow (GitHub Actions, aarch64 + x86_64)
  - Homebrew formula (SHA-256 placeholder pending first release)
  - Library/binary crate split with doc tests
- Partially done: None
- Not done: None (future items: JSON output, shell completions, Nix package, config wizard)

## Next best task

Implementation is complete. Release by tagging:

    git tag v0.1.0
    git push --tags

Then update `packaging/summon.rb` with the real SHA-256 from the release artifacts.

## Blockers / open questions

- None known

## Notes for next agent

- The library crate (`lib.rs`) exposes the public API: app, config, controller, diagnostics
- The binary crate (`main.rs`) is a thin CLI dispatch wrapper with `mod cli`
- The `FakeAppController` doc example is verified by `cargo test --doc`
- The release workflow uses `softprops/action-gh-release@v2` to create GitHub releases automatically on tag push
- The Homebrew formula at `packaging/summon.rb` has a placeholder SHA-256 that must be replaced after the first release is published
- The target directory is configured on an external SSD via CARGO_TARGET_DIR or .cargo/config.toml
- Integration tests that interact with real macOS apps (Finder launch, Accessibility checks) are slow (~2 min total) because they wait for AppleScript timeouts
