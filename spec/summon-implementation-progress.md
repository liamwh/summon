# Summon implementation progress

## Last updated

- Commit: 0f202fa (completion audit)
- Date: 2026-06-05
- Agent task: Completion audit — verify all spec items against git truth

## What changed in this iteration

- Performed a systematic completion audit against every criterion in the agent prompt
- Verified 133 unit tests + 14 integration tests pass (clippy clean, fmt clean)
- Confirmed all MVP spec items are implemented and tested
- Corrected stale timing note: full test suite runs in ~4 minutes, not 22

## Verification run

- `cargo test --workspace` — 133 unit tests passed, 14 integration tests passed (~4 min total)
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo build --release -p summon` — succeeded, 752KB stripped arm64 binary

## Completion audit

All spec criteria verified against git:

- summon <binding> — wired with config → binding → action → controller
- summon app <app> — classifies target, uses MacAppController
- summon list — prints configured bindings, integration tested
- summon config path — prints XDG path, integration tested
- summon config check — validates TOML, integration tested
- summon doctor — checks config, bindings, accessibility
- XDG config resolution — unit tested (XDG, fallback, HOME)
- TOML parsing/validation — 20+ tests (unknown fields, empty apps, invalid strategies)
- Missing config errors — "Could not read config file" with path
- Missing binding errors — lists available bindings
- Invalid config errors — parse/validation with specifics
- Bundle ID / app name / app path targets — AppTarget enum
- Launch-if-not-running / focus-if-running / cycle-when-focused — decide_action + controller
- Accessibility documented — doctor, error messages, README
- No live GUI in tests — FakeAppController
- Deterministic decision logic — 133 unit tests
- Hyper+F and Hyper+Z examples — skhdrc
- External hotkey examples — skhd, Raycast, shell aliases

## Current state reconstructed from git

- Done: All v1 MVP spec items implemented and tested
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

- The release workflow uses `softprops/action-gh-release@v2` to create GitHub releases automatically on tag push
- The Homebrew formula at `packaging/summon.rb` has a placeholder SHA-256 that must be replaced after the first release is published
- Full test suite runs in ~4 minutes (not 22 — previous estimate was stale)
- The target directory is configured on an external SSD via CARGO_TARGET_DIR or .cargo/config.toml
