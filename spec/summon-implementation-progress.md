# Summon implementation progress

## Last updated

- Commit: ee7cb8b (completion audit)
- Date: 2026-06-05
- Agent task: Add lib.rs for library/binary crate split

## What changed in this iteration

- Split the crate into `lib.rs` (public API) + `main.rs` (binary entry point)
- Created `lib.rs` with public module exports (app, config, controller, diagnostics)
- Slimmed `main.rs` to CLI dispatch only
- Updated `cli.rs` imports from `crate::` to `summon::`
- Fixed `missing_docs` warning on binary crate
- Doc test for `FakeAppController` now runs and passes

## Verification run

- `cargo test --workspace` — 122 library + 11 binary + 14 integration tests passed, 1 doc test passed
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean
- `cargo test --doc -p summon` — 1 doc test passed (FakeAppController example)

## Current state reconstructed from git

- Done: All v1 MVP spec items implemented and tested
- Done: Library/binary crate split with runnable doc tests
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
- The `FakeAppController` doc example is now verified by `cargo test --doc`
- The release workflow uses `softprops/action-gh-release@v2` to create GitHub releases automatically on tag push
- The Homebrew formula at `packaging/summon.rb` has a placeholder SHA-256 that must be replaced after the first release is published
- The target directory is configured on an external SSD via CARGO_TARGET_DIR or .cargo/config.toml
