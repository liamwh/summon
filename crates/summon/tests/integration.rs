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
fn app_command_succeeds_with_bundle_id() {
    let output = Command::new(summon_bin())
        .args(["app", "com.apple.finder"])
        .output()
        .expect("should run summon");

    assert!(
        output.status.success(),
        "app command should succeed with Finder bundle ID: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn app_command_rejects_invalid_path() {
    let output = Command::new(summon_bin())
        .args(["app", "/Applications/notanapp"])
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "app command should fail for invalid app path"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid app target"),
        "stderr should mention invalid app target: {stderr}"
    );
    assert!(
        stderr.contains(".app"),
        "stderr should mention .app extension: {stderr}"
    );
}

#[test]
fn list_command_succeeds_with_config() {
    let dir = std::env::temp_dir().join("summon_test_list_with_config");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.finder]\napp = \"com.apple.finder\"\n\n[bindings.browser]\napp = \"com.brave.Browser\"\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["list"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        output.status.success(),
        "list should succeed with valid config: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("browser"),
        "output should list browser: {stdout}"
    );
    assert!(
        stdout.contains("finder"),
        "output should list finder: {stdout}"
    );
    assert!(
        stdout.contains("com.apple.finder"),
        "output should show app target: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn list_command_reports_missing_config() {
    let dir = std::env::temp_dir().join("summon_test_list_missing_config");
    std::fs::create_dir_all(&dir).unwrap();

    let output = Command::new(summon_bin())
        .args(["list"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "list should fail when config is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not read config file"),
        "stderr should mention missing file: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn doctor_command_runs_diagnostics() {
    let dir = std::env::temp_dir().join("summon_test_doctor_integration");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.finder]\napp = \"com.apple.finder\"\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["doctor"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Summon doctor"),
        "should print doctor header: {stdout}"
    );
    assert!(
        stdout.contains("Config path:"),
        "should print config path: {stdout}"
    );
    assert!(
        stdout.contains("Accessibility:"),
        "should check accessibility: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn doctor_command_reports_missing_config() {
    let dir = std::env::temp_dir().join("summon_test_doctor_missing_integration");
    std::fs::create_dir_all(&dir).unwrap();

    let output = Command::new(summon_bin())
        .args(["doctor"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "doctor should fail when config is missing"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("not found"),
        "should mention config not found: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// summon <binding> — core command path
// ---------------------------------------------------------------------------

#[test]
fn binding_command_succeeds_with_valid_config() {
    let dir = std::env::temp_dir().join("summon_test_binding_valid");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[settings]\nlaunch_if_not_running = true\n\n[bindings.finder]\napp = \"com.apple.finder\"\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["finder"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        output.status.success(),
        "binding should succeed with valid config: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn binding_command_reports_missing_config() {
    let dir = std::env::temp_dir().join("summon_test_binding_missing_config");
    std::fs::create_dir_all(&dir).unwrap();

    let output = Command::new(summon_bin())
        .args(["terminal"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "binding should fail when config is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not read config file"),
        "stderr should mention missing file: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn binding_command_reports_unknown_binding() {
    let dir = std::env::temp_dir().join("summon_test_binding_unknown");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.browser]\napp = \"com.brave.Browser\"\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["terminal"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "binding should fail for unknown binding name"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not resolve binding: terminal"),
        "stderr should mention unknown binding: {stderr}"
    );
    assert!(
        stderr.contains("browser"),
        "stderr should suggest available bindings: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn binding_command_reports_invalid_config() {
    let dir = std::env::temp_dir().join("summon_test_binding_invalid_config");
    let summon_dir = dir.join("summon");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[bindings.broken]\ncycle_when_focused = true\n",
    )
    .unwrap();

    let output = Command::new(summon_bin())
        .args(["broken"])
        .env("XDG_CONFIG_HOME", &dir)
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "binding should fail with invalid config"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Config error"),
        "stderr should mention config error: {stderr}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
