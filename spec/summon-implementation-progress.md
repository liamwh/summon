# Summon implementation progress

## Last updated

- Commit: 66cd04d (pending new commit)
- Date: 2026-06-05
- Agent task: Add packaging and release support

## What changed in this iteration

- Added `[profile.release]` to workspace Cargo.toml with `strip = true`, `lto = true`, `codegen-units = 1`, `panic = "abort"` — produces a 752KB stripped arm64 binary
- Added `.github/workflows/release.yml` — triggers on `v*` tags, builds for `aarch64-apple-darwin` (macos-latest) and `x86_64-apple-darwin` (macos-13), creates a GitHub release with tarballs and SHA-256 checksums
- Added `packaging/summon.rb` — Homebrew formula template for a `liamwh/tap/summon` tap

## Verification run

- `cargo test --workspace` — 14 passed (integration), unit tests embedded in source files also passed
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo build --release -p summon` — succeeded, 752KB stripped arm64 binary

## Current state reconstructed from git

- Done:
  - Rust workspace with single `summon` crate
  - CLI definition via clap with all planned subcommands
  - Config module with full TOML model, path resolution, parsing, validation
  - Binding lookup and effective settings resolution
  - App target resolution with `AppTarget` enum and `classify_app_target()`
  - `AppController` trait with `FakeAppController` for deterministic testing
  - `decide_action()` pure decision logic (launch/focus/cycle/noop)
  - `execute_action()` dispatches decided action against controller
  - `MacAppController` — real macOS launch/focus via `open` and `osascript`
  - `MacAppController::cycle_window` — real macOS window cycling via AppleScript/System Events
  - `summon <binding>` core path wired with real macOS controller
  - `summon app <app>` direct app targeting wired with real macOS controller
  - CLI dispatch: `summon config path`, `summon config check`, `summon list`, `summon <binding>`, `summon app <app>`, `summon doctor` — all wired
  - Diagnostics module with config, binding, and accessibility checks
  - Workspace lint configuration (clippy pedantic, missing_docs, unwrap/expect warnings)
  - README with installation and usage
  - Integration test suite (14 tests)
  - Example configs for skhd, Raycast, shell aliases
  - GitHub Actions CI pipeline
  - Release profile in Cargo.toml
  - GitHub Actions release workflow (dual-arch Apple Silicon + Intel)
  - Homebrew formula template
- Partially done:
  - None
- Not done:
  - None (all spec items implemented)

## Next best task

Implementation is complete. The next action is to tag a release (e.g. `git tag v0.1.0 && git push --tags`) which will trigger the release workflow.

## Blockers / open questions

- None known

## Notes for next agent

- The release workflow uses `softprops/action-gh-release@v2` to create GitHub releases automatically on tag push
- The Homebrew formula at `packaging/summon.rb` has a placeholder SHA-256 that must be replaced after the first release is published
- Integration tests that call real macOS commands are slow (~22 minutes total for 14 tests) due to `osascript` calls
- The target directory is configured on an external SSD via CARGO_TARGET_DIR or .cargo/config.toml
