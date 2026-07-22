#![cfg(windows)]

use dennett_observability::{ExitStatus, MarkerState, inspect_local};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const COMPONENT: &str = "dennett-node";

#[test]
fn detached_node_persists_handled_startup_failure_without_private_content() {
    let temp = tempfile::tempdir().expect("temporary Node profile");
    let status = Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env("DENNETT_DATA_DIR", temp.path())
        .env_remove("DENNETT_INSTALLATION_ID")
        .env("DENNETT_AGENT_RUNTIME", "fake")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run invalid detached Node");
    assert!(!status.success());

    let summary = inspect_local(temp.path(), COMPONENT).expect("diagnostic summary");
    assert_eq!(summary.previous_exit, ExitStatus::Failed);
    assert_eq!(
        summary.previous_error_code.as_deref(),
        Some("node.config.installation_missing")
    );
    assert_eq!(summary.active_runs.len(), 0);
    let logs = read_logs(temp.path());
    assert!(logs.contains("node.configuration_failed"));
    assert!(logs.contains("node.config.installation_missing"));
    assert!(!logs.contains("DENNETT_INSTALLATION_ID is required"));
}

#[test]
fn detached_node_restart_identifies_an_unclean_previous_process() {
    let temp = tempfile::tempdir().expect("temporary Node profile");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("project directory");
    let installation = format!("diagnostic-{}", uuid::Uuid::now_v7().simple());

    let mut first = spawn_node(temp.path(), &project, &installation);
    wait_for_log(&mut first, temp.path(), "node.ipc_start_requested");
    first.kill().expect("terminate first Node");
    first.wait().expect("reap first Node");
    let interrupted = inspect_local(temp.path(), COMPONENT).expect("interrupted summary");
    assert_eq!(interrupted.active_runs.len(), 1);
    assert_eq!(interrupted.active_runs[0].marker_state, MarkerState::Stale);

    let mut second = spawn_node(temp.path(), &project, &installation);
    wait_for_log(&mut second, temp.path(), "\"status\":\"unclean\"");
    second.kill().expect("terminate second Node");
    second.wait().expect("reap second Node");

    let logs = read_logs(temp.path());
    assert!(logs.contains("\"status\":\"unclean\""));
    assert!(!logs.contains(project.to_string_lossy().as_ref()));
}

#[test]
fn unavailable_log_directory_does_not_prevent_node_startup() {
    let temp = tempfile::tempdir().expect("temporary Node profile");
    std::fs::write(
        temp.path().join("diagnostics"),
        b"blocks diagnostic directory",
    )
    .expect("diagnostic blocker");
    let project = temp.path().join("project");
    std::fs::create_dir_all(&project).expect("project directory");
    let installation = format!("diagnostic-{}", uuid::Uuid::now_v7().simple());

    let mut child = spawn_node(temp.path(), &project, &installation);
    wait_for_path(&mut child, &temp.path().join("control.sqlite3"));
    assert_eq!(child.try_wait().expect("inspect Node"), None);
    child.kill().expect("terminate Node");
    child.wait().expect("reap Node");
}

#[test]
fn adapter_host_safe_stderr_code_is_preserved_without_provider_details() {
    let temp = tempfile::tempdir().expect("temporary Node profile");
    let script = write_diagnostic_host(
        temp.path(),
        r#"{"v":1,"diagnosticCode":"runtime_host.unhandled_failure"}"#,
    );
    let status = run_node_with_host(temp.path(), &script);
    assert!(!status.success());
    let logs = read_logs(temp.path());
    assert!(logs.contains("runtime.host_unhandled_failure"));
    assert!(logs.contains("runtime_host.unhandled_failure"));
}

#[test]
fn adapter_host_raw_stderr_is_classified_without_copying_a_secret() {
    const SECRET: &str = "sk-proj-private-provider-secret";
    let temp = tempfile::tempdir().expect("temporary Node profile");
    let script = write_diagnostic_host(temp.path(), SECRET);
    let status = run_node_with_host(temp.path(), &script);
    assert!(!status.success());
    let logs = read_logs(temp.path());
    assert!(logs.contains("runtime.host_stderr_unclassified"));
    assert!(!logs.contains(SECRET));
}

fn spawn_node(data_dir: &Path, project: &Path, installation: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env("DENNETT_DATA_DIR", data_dir)
        .env("DENNETT_INSTALLATION_ID", installation)
        .env("DENNETT_AUTHORITY_EPOCH", "1")
        .env("DENNETT_PROJECT_ROOT", project)
        .env("DENNETT_AGENT_RUNTIME", "fake")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn detached Node")
}

fn run_node_with_host(data_dir: &Path, script: &Path) -> std::process::ExitStatus {
    let project = data_dir.join("project");
    std::fs::create_dir_all(&project).expect("project directory");
    Command::new(env!("CARGO_BIN_EXE_dennett-node"))
        .env("DENNETT_DATA_DIR", data_dir)
        .env(
            "DENNETT_INSTALLATION_ID",
            format!("diagnostic-{}", uuid::Uuid::now_v7().simple()),
        )
        .env("DENNETT_AUTHORITY_EPOCH", "1")
        .env("DENNETT_PROJECT_ROOT", project)
        .env("DENNETT_AGENT_RUNTIME", "codex")
        .env("DENNETT_RUNTIME_HOST_SCRIPT", script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run Node with diagnostic host")
}

fn write_diagnostic_host(directory: &Path, diagnostic_line: &str) -> std::path::PathBuf {
    let script = directory.join("diagnostic-host.mjs");
    let encoded = serde_json::to_string(diagnostic_line).expect("diagnostic fixture string");
    std::fs::write(
        &script,
        format!(
            r#"
import readline from "node:readline";
process.stderr.write({encoded} + "\n");
const write = value => process.stdout.write(JSON.stringify(value) + "\n");
readline.createInterface({{ input: process.stdin }}).on("line", line => {{
  const request = JSON.parse(line);
  if (request.method === "health") {{
    write({{ v: 1, id: request.id, result: {{ status: "healthy", protocolVersion: 1 }} }});
  }} else if (request.method === "describe") {{
    write({{ v: 1, id: request.id, result: {{}} }});
  }}
}});
"#
        ),
    )
    .expect("write diagnostic host");
    script.canonicalize().expect("canonical diagnostic host")
}

fn wait_for_log(child: &mut Child, data_dir: &Path, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if read_logs(data_dir).contains(needle) {
            return;
        }
        if let Some(status) = child.try_wait().expect("inspect child") {
            panic!("Node exited before diagnostic checkpoint: {status}");
        }
        assert!(Instant::now() < deadline, "diagnostic checkpoint timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_path(child: &mut Child, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if path.is_file() {
            return;
        }
        if let Some(status) = child.try_wait().expect("inspect child") {
            panic!("Node exited before creating {}: {status}", path.display());
        }
        assert!(Instant::now() < deadline, "Node startup timed out");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn read_logs(data_dir: &Path) -> String {
    let directory = data_dir.join("diagnostics/logs");
    let Ok(entries) = std::fs::read_dir(directory) else {
        return String::new();
    };
    let mut output = String::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
            && let Ok(log) = std::fs::read_to_string(path)
        {
            output.push_str(&log);
        }
    }
    output
}
