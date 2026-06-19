//! Integration tests for Summon CLI commands.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the path to the summon binary built by `cargo build`.
fn summon_bin() -> String {
    env!("CARGO_BIN_EXE_summon").to_string()
}

/// Returns a summon command pinned to direct mode for deterministic tests.
fn summon_cmd() -> Command {
    let mut command = Command::new(summon_bin());
    command.env("SUMMON_DAEMON", "off");
    command
}

fn unique_test_dir(label: &str) -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    std::env::temp_dir().join(format!("summon_{label}_{suffix}"))
}

// ---------------------------------------------------------------------------
// summon config path
// ---------------------------------------------------------------------------

#[test]
fn config_path_prints_a_path() {
    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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
// App, list, doctor, and binding command tests
// ---------------------------------------------------------------------------

#[test]
fn app_command_reports_missing_bundle_id() {
    let output = summon_cmd()
        .args(["app", "com.example.summon-missing-test-app"])
        .output()
        .expect("should run summon");

    assert!(
        !output.status.success(),
        "app command should fail for a missing bundle ID"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not launch com.example.summon-missing-test-app"),
        "stderr should mention launch failure: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn app_command_rejects_invalid_path() {
    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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
        stdout.contains("Accessibility (AXIsProcessTrusted):"),
        "should check accessibility: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn doctor_command_reports_missing_config() {
    let dir = std::env::temp_dir().join("summon_test_doctor_missing_integration");
    std::fs::create_dir_all(&dir).unwrap();

    let output = summon_cmd()
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
        "[settings]\nlaunch_if_not_running = false\n\n[bindings.finder]\napp = \"com.apple.finder\"\n",
    )
    .unwrap();

    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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

    let output = summon_cmd()
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

// ---------------------------------------------------------------------------
// summon daemon
// ---------------------------------------------------------------------------

#[test]
fn daemon_run_status_and_stop() {
    let root = unique_test_dir("daemon");
    let config_root = root.join("xdg");
    let summon_dir = config_root.join("summon");
    let socket_path = root.join("summond.sock");
    std::fs::create_dir_all(&summon_dir).unwrap();

    std::fs::write(
        summon_dir.join("summon.toml"),
        "[settings]\nlaunch_if_not_running = false\n\n[bindings.missing]\napp = \"com.example.summon-daemon-missing\"\n",
    )
    .unwrap();

    let mut daemon = Command::new(summon_bin())
        .args(["daemon", "run"])
        .env("SUMMOND_SOCKET_PATH", &socket_path)
        .env("XDG_CONFIG_HOME", &config_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("should spawn daemon");

    let mut ready = false;
    for _ in 0..40 {
        let status = Command::new(summon_bin())
            .args(["daemon", "status"])
            .env("SUMMOND_SOCKET_PATH", &socket_path)
            .output()
            .expect("should inspect daemon status");
        if status.status.success() {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ready, "daemon should become ready");

    let binding = Command::new(summon_bin())
        .args(["missing"])
        .env("SUMMON_DAEMON", "required")
        .env("SUMMOND_SOCKET_PATH", &socket_path)
        .env("XDG_CONFIG_HOME", &config_root)
        .output()
        .expect("should run binding through daemon");
    assert!(
        binding.status.success(),
        "binding through daemon should succeed: stderr={}",
        String::from_utf8_lossy(&binding.stderr)
    );

    let status = Command::new(summon_bin())
        .args(["daemon", "status"])
        .env("SUMMOND_SOCKET_PATH", &socket_path)
        .output()
        .expect("should inspect daemon status");
    assert!(
        status.status.success(),
        "daemon status should succeed: stderr={}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("Summon daemon: running"),
        "status should report running: {}",
        String::from_utf8_lossy(&status.stdout)
    );

    let stop = Command::new(summon_bin())
        .args(["daemon", "stop"])
        .env("SUMMOND_SOCKET_PATH", &socket_path)
        .output()
        .expect("should stop daemon");
    assert!(
        stop.status.success(),
        "daemon stop should succeed: stderr={}",
        String::from_utf8_lossy(&stop.stderr)
    );

    for _ in 0..40 {
        if daemon
            .try_wait()
            .expect("should poll daemon child")
            .is_some()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();

    std::fs::remove_dir_all(&root).ok();
}
