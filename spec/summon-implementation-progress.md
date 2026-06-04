# Summon implementation progress

## Last updated

- Commit: d0fedc1 (pending new commit)
- Date: 2026-06-04
- Agent task: Add GitHub Actions CI pipeline

## What changed in this iteration

- Added `.github/workflows/ci.yml` — runs fmt, clippy, and tests on push to main and on pull requests
- Uses `macos-latest` runner since Summon is a macOS-specific tool
- Uses `dtolnay/rust-toolchain@stable` with rustfmt and clippy components
- Uses `Swatinem/rust-cache@v2` for Cargo build caching

## Verification run

- `cargo test --workspace` — 140 passed (126 unit + 14 integration), 0 failed
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

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
  - `summon <binding>` core path wired with real macOS controller
  - `summon app <app>` direct app targeting wired with real macOS controller
  - CLI dispatch: `summon config path`, `summon config check`, `summon list`, `summon <binding>`, `summon app <app>`, `summon doctor` — all wired
  - Diagnostics module with config, binding, and accessibility checks
  - Workspace lint configuration (clippy pedantic, missing_docs, unwrap/expect warnings)
  - README with installation and usage
  - Integration test suite (14 tests)
  - Example configs for skhd, Raycast, shell aliases
  - GitHub Actions CI pipeline
- Partially done:
  - `cycle_window` on MacAppController is a no-op (graceful degradation, not a real implementation)
- Not done:
  - Window cycling via macOS Accessibility API
  - Packaging and release (release profile, GitHub Actions release build, Homebrew tap, binary artefacts)

## Next best task

Implement window cycling via the macOS Accessibility API. This is the most significant functional gap — `cycle_window` is currently a no-op. Real cycling requires listing windows for a target app, detecting the current window, and selecting the next one.

## Blockers / open questions

- None known

## Notes for next agent

- All CLI commands are fully wired: `summon <binding>`, `summon app <app>`, `summon list`, `summon config path`, `summon config check`, and `summon doctor`.
- The `app_command_succeeds_with_bundle_id` and `binding_command_succeeds_with_valid_config` integration tests are slow (~60s each) because they call real macOS `osascript`. Similarly, the doctor integration tests call `osascript` for the accessibility check.
- The CI pipeline runs on `macos-latest` so osascript-dependent tests work in CI.
- The `cycle_window` method on `MacAppController` is a no-op. Real window cycling requires the macOS Accessibility API.
- Example files are in `examples/` — they are pure documentation, not Rust integration tests or build targets.
