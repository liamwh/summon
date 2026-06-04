//! Integration tests for Summon CLI commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::Command;

/// Returns the path to the summon binary built by `cargo build`.
fn summon_bin() -> String {
    env!("CARGO_BIN_EXE_summon").to_string()
}

// ---------------------------------------------------------------------------
// summon config path
// ---------------------------------------------------------------------------

#[test]
fn config_path_prints_a_path() {
    let output = Command::new(summon_bin())
        .args(["config", "path"])
        .output()
        .expect("should run summon");

    assert!(
        output.status.success(),
        "config path should succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("summon.toml"),
        "output should contain summon.toml: {stdout}"
    );
    assert!(
        stdout.contains("summon"),
        "output should contain summon dir: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// summon config check — missing config
// ---------------------------------------------------------------------------

#[test]
fn config_check_reports_missing_config() {
    let dir = std::env::temp_dir().join("summon_test_config_check_missing");
    std::fs::create_dir_all(&dir).unwrap();

    let output = Command::new(summon_bin())
        .args(["config", "check"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "config check should fail when config is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not read config file"),
        "stderr should mention missing file: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// summon config check — valid config
// ---------------------------------------------------------------------------

#[test]
fn config_check_succeeds_with_valid_config() {
    let dir = std::env::temp_dir().join("summon_test_config_check_valid");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.finder]\napp = \"com.apple.finder\"\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["config", "check"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        output.status.success(),
        "config check should succeed with valid config: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Config is valid"),
        "stdout should say config is valid: {stdout}"
    );
    assert!(
        stdout.contains("1 binding(s)"),
        "stdout should report binding count: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// summon config check — invalid config
// ---------------------------------------------------------------------------

#[test]
fn config_check_reports_invalid_config() {
    let dir = std::env::temp_dir().join("summon_test_config_check_invalid");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.broken]\ncycle_when_focused = true\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["config", "check"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "config check should fail with invalid config"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Config error"),
        "stderr should mention config error: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Unimplemented commands
// ---------------------------------------------------------------------------

#[test]
fn unimplemented_app_command_fails() {
    let output = Command::new(summon_bin())
        .args(["app", "Ghostty"])
        .output()
        .expect("should run summon");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "should say not yet implemented: {stderr}"
    );
}

#[test]
fn unimplemented_list_command_fails() {
    let output = Command::new(summon_bin())
        .args(["list"])
        .output()
        .expect("should run summon");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "should say not yet implemented: {stderr}"
    );
}

#[test]
fn unimplemented_doctor_command_fails() {
    let output = Command::new(summon_bin())
        .args(["doctor"])
        .output()
        .expect("should run summon");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "should say not yet implemented: {stderr}"
    );
}

#[test]
fn unimplemented_binding_command_fails() {
    let output = Command::new(summon_bin())
        .args(["terminal"])
        .output()
        .expect("should run summon");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "should say not yet implemented: {stderr}"
    );
    assert!(
        stderr.contains("terminal"),
        "should mention the binding name: {stderr}"
    );
}
